mod agent_planner;
mod clawpal_server;
mod legacy_doctor;
mod shared;

use serde_json::json;
use tauri::{AppHandle, Runtime, State};
use uuid::Uuid;

pub(crate) use self::agent_planner::run_agent_planner_repair_loop;
pub(crate) use self::clawpal_server::run_clawpal_server_repair_loop;
pub(crate) use self::legacy_doctor::run_remote_doctor_repair_loop;
use self::shared::is_unknown_method_error;
use super::agent::{
    build_agent_plan_prompt, configured_remote_doctor_protocol, default_remote_doctor_protocol,
    detect_method_name, ensure_agent_workspace_ready, gateway_url_is_local,
    protocol_requires_bridge, remote_doctor_agent_id, remote_doctor_agent_session_key,
    repair_method_name,
};
use super::config::{
    build_gateway_credentials, empty_config_excerpt_context, empty_diagnosis, load_gateway_config,
};
use super::legacy::{
    ensure_agent_bridge_connected, ensure_remote_target_connected, parse_agent_plan_response,
    run_agent_request_with_bridge,
};
use super::plan::request_plan;
use super::session::append_session_log;
use super::types::{parse_target_location, PlanKind, RemoteDoctorProtocol, RemoteDoctorRepairResult, TargetLocation};
use crate::bridge_client::BridgeClient;
use crate::commands::logs::log_dev;
use crate::node_client::NodeClient;
use crate::ssh::SshConnectionPool;

pub(crate) async fn start_remote_doctor_repair_impl<R: Runtime>(
    app: AppHandle<R>,
    pool: &SshConnectionPool,
    instance_id: String,
    target_location: String,
) -> Result<RemoteDoctorRepairResult, String> {
    let target_location = parse_target_location(&target_location)?;
    if matches!(target_location, TargetLocation::RemoteOpenclaw) {
        ensure_remote_target_connected(pool, &instance_id).await?;
    }
    let session_id = Uuid::new_v4().to_string();
    let gateway = load_gateway_config()?;
    let creds = build_gateway_credentials(gateway.auth_token_override.as_deref())?;
    log_dev(format!(
        "[remote_doctor] start session={} instance_id={} target_location={:?} gateway_url={} auth_token_override={}",
        session_id,
        instance_id,
        target_location,
        gateway.url,
        gateway.auth_token_override.is_some()
    ));
    append_session_log(
        &session_id,
        json!({
            "event": "session_start",
            "instanceId": instance_id,
            "targetLocation": target_location,
            "gatewayUrl": gateway.url,
            "gatewayAuthTokenOverride": gateway.auth_token_override.is_some(),
        }),
    );

    let client = NodeClient::new();
    client.connect(&gateway.url, app.clone(), creds).await?;
    let bridge = BridgeClient::new();

    let forced_protocol = configured_remote_doctor_protocol();
    let active_protocol = forced_protocol.unwrap_or(default_remote_doctor_protocol());
    let pool_ref: &SshConnectionPool = pool;
    let app_handle = app.clone();
    let bridge_client = bridge.clone();
    let gateway_url = gateway.url.clone();
    let gateway_auth_override = gateway.auth_token_override.clone();
    if matches!(active_protocol, RemoteDoctorProtocol::AgentPlanner)
        && gateway_url_is_local(&gateway_url)
    {
        ensure_agent_workspace_ready()?;
    }
    if protocol_requires_bridge(active_protocol) {
        ensure_agent_bridge_connected(
            &app,
            &bridge,
            &gateway_url,
            gateway_auth_override.as_deref(),
            &session_id,
        )
        .await;
    }
    let result = match active_protocol {
        RemoteDoctorProtocol::AgentPlanner => {
            let agent = run_agent_planner_repair_loop(
                &app,
                &client,
                &bridge_client,
                pool_ref,
                &session_id,
                &instance_id,
                target_location,
            )
            .await;

            if forced_protocol.is_none()
                && matches!(&agent, Err(error) if is_unknown_method_error(error))
            {
                append_session_log(
                    &session_id,
                    json!({
                        "event": "protocol_fallback",
                        "from": "agent",
                        "to": "legacy_doctor",
                        "reason": agent.as_ref().err(),
                    }),
                );
                run_remote_doctor_repair_loop(
                    Some(&app),
                    pool_ref,
                    &session_id,
                    &instance_id,
                    target_location,
                    |kind, round, previous_results| {
                        let method = match kind {
                            PlanKind::Detect => detect_method_name(),
                            PlanKind::Investigate => repair_method_name(),
                            PlanKind::Repair => repair_method_name(),
                        };
                        let client = &client;
                        let session_id = &session_id;
                        let instance_id = &instance_id;
                        async move {
                            request_plan(
                                client,
                                &method,
                                kind,
                                session_id,
                                round,
                                target_location,
                                instance_id,
                                &previous_results,
                            )
                            .await
                        }
                    },
                )
                .await
            } else {
                agent
            }
        }
        RemoteDoctorProtocol::LegacyDoctor => {
            let legacy = run_remote_doctor_repair_loop(
                Some(&app),
                pool_ref,
                &session_id,
                &instance_id,
                target_location,
                |kind, round, previous_results| {
                    let method = match kind {
                        PlanKind::Detect => detect_method_name(),
                        PlanKind::Investigate => repair_method_name(),
                        PlanKind::Repair => repair_method_name(),
                    };
                    let client = &client;
                    let session_id = &session_id;
                    let instance_id = &instance_id;
                    async move {
                        request_plan(
                            client,
                            &method,
                            kind,
                            session_id,
                            round,
                            target_location,
                            instance_id,
                            &previous_results,
                        )
                        .await
                    }
                },
            )
            .await;

            if forced_protocol.is_none()
                && matches!(&legacy, Err(error) if is_unknown_method_error(error))
            {
                append_session_log(
                    &session_id,
                    json!({
                        "event": "protocol_fallback",
                        "from": "legacy_doctor",
                        "to": "clawpal_server",
                        "reason": legacy.as_ref().err(),
                    }),
                );
                log_dev(format!(
                    "[remote_doctor] session={} protocol fallback legacy_doctor -> clawpal_server",
                    session_id
                ));
                run_clawpal_server_repair_loop(
                    &app,
                    &client,
                    &session_id,
                    &instance_id,
                    target_location,
                )
                .await
            } else {
                legacy
            }
        }
        RemoteDoctorProtocol::ClawpalServer => {
            let clawpal_server = run_clawpal_server_repair_loop(
                &app,
                &client,
                &session_id,
                &instance_id,
                target_location,
            )
            .await;
            if forced_protocol.is_none()
                && matches!(&clawpal_server, Err(error) if is_unknown_method_error(error))
            {
                append_session_log(
                    &session_id,
                    json!({
                        "event": "protocol_fallback",
                        "from": "clawpal_server",
                        "to": "agent",
                        "reason": clawpal_server.as_ref().err(),
                    }),
                );
                let agent = run_remote_doctor_repair_loop(
                    Some(&app),
                    pool_ref,
                    &session_id,
                    &instance_id,
                    target_location,
                    |kind, round, previous_results| {
                        let client = &client;
                        let session_id = &session_id;
                        let instance_id = &instance_id;
                        let app_handle = app_handle.clone();
                        let bridge_client = bridge_client.clone();
                        let gateway_url = gateway_url.clone();
                        let gateway_auth_override = gateway_auth_override.clone();
                        let empty_diagnosis = empty_diagnosis();
                        let empty_config = empty_config_excerpt_context();
                        async move {
                            ensure_agent_bridge_connected(
                                &app_handle,
                                &bridge_client,
                                &gateway_url,
                                gateway_auth_override.as_deref(),
                                session_id,
                            )
                            .await;
                            let text = if bridge_client.is_connected().await {
                                run_agent_request_with_bridge(
                                    &app_handle,
                                    client,
                                    &bridge_client,
                                    pool_ref,
                                    target_location,
                                    instance_id,
                                    remote_doctor_agent_id(),
                                    &remote_doctor_agent_session_key(session_id),
                                    &build_agent_plan_prompt(
                                        kind,
                                        session_id,
                                        round,
                                        target_location,
                                        instance_id,
                                        &empty_diagnosis,
                                        &empty_config,
                                        &previous_results,
                                    ),
                                )
                                .await?
                            } else {
                                client
                                    .run_agent_request(
                                        remote_doctor_agent_id(),
                                        &remote_doctor_agent_session_key(session_id),
                                        &build_agent_plan_prompt(
                                            kind,
                                            session_id,
                                            round,
                                            target_location,
                                            instance_id,
                                            &empty_diagnosis,
                                            &empty_config,
                                            &previous_results,
                                        ),
                                    )
                                    .await?
                            };
                            parse_agent_plan_response(kind, &text)
                        }
                    },
                )
                .await;
                if matches!(&agent, Err(error) if is_unknown_method_error(error)) {
                    append_session_log(
                        &session_id,
                        json!({
                            "event": "protocol_fallback",
                            "from": "agent",
                            "to": "legacy_doctor",
                            "reason": agent.as_ref().err(),
                        }),
                    );
                    run_remote_doctor_repair_loop(
                        Some(&app),
                        pool_ref,
                        &session_id,
                        &instance_id,
                        target_location,
                        |kind, round, previous_results| {
                            let method = match kind {
                                PlanKind::Detect => detect_method_name(),
                                PlanKind::Investigate => repair_method_name(),
                                PlanKind::Repair => repair_method_name(),
                            };
                            let client = &client;
                            let session_id = &session_id;
                            let instance_id = &instance_id;
                            async move {
                                request_plan(
                                    client,
                                    &method,
                                    kind,
                                    session_id,
                                    round,
                                    target_location,
                                    instance_id,
                                    &previous_results,
                                )
                                .await
                            }
                        },
                    )
                    .await
                } else {
                    agent
                }
            } else {
                clawpal_server
            }
        }
    };

    let _ = client.disconnect().await;
    let _ = bridge.disconnect().await;

    match result {
        Ok(done) => {
            append_session_log(
                &session_id,
                json!({
                    "event": "session_complete",
                    "status": "completed",
                    "latestDiagnosisHealthy": done.latest_diagnosis_healthy,
                }),
            );
            Ok(done)
        }
        Err(error) => {
            append_session_log(
                &session_id,
                json!({
                    "event": "session_complete",
                    "status": "failed",
                    "reason": error,
                }),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn start_remote_doctor_repair(
    app: AppHandle,
    pool: State<'_, SshConnectionPool>,
    instance_id: String,
    target_location: String,
) -> Result<RemoteDoctorRepairResult, String> {
    start_remote_doctor_repair_impl(app, &pool, instance_id, target_location).await
}
