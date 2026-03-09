import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";

import i18n from "@/i18n";
import {
  applySshConfigSuggestionToForm,
  buildSshFormSubmission,
  dedupeAndSortSshConfigHosts,
  formatSshConfigSuggestionLabel,
  resolveSshConfigPresetSelection,
  SshFormWidget,
  SSH_CONFIG_MANUAL_ALIAS,
  submitSshForm,
} from "../SshFormWidget";

describe("SshFormWidget layout", () => {
  test("uses responsive form layout for host and port fields", async () => {
    await i18n.changeLanguage("en");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(SshFormWidget, {
          invokeId: "connect-ssh-form",
          onSubmit: () => {},
          onCancel: () => {},
        }),
      }),
    );

    expect(html).toContain("grid-cols-1");
    expect(html).toContain("sm:grid-cols-3");
    expect(html).toContain("min-w-0");
  });

  test("renders the ssh config preset section when suggestions exist", async () => {
    await i18n.changeLanguage("en");

    const html = renderToStaticMarkup(
      React.createElement(I18nextProvider, {
        i18n,
        children: React.createElement(SshFormWidget, {
          invokeId: "connect-ssh-form",
          sshConfigSuggestions: [
            { hostAlias: "prod", hostName: "Production", user: "root", port: 2222 },
          ],
          onSubmit: () => {},
          onCancel: () => {},
        }),
      }),
    );

    expect(html).toContain("SSH Config Host");
    expect(html).toContain("Selecting an alias fills host alias, username, port, and key path.");
  });
});

describe("dedupeAndSortSshConfigHosts", () => {
  test("drops blank aliases, deduplicates exact aliases, and sorts case-insensitively", () => {
    expect(
      dedupeAndSortSshConfigHosts([
        { hostAlias: "  " },
        { hostAlias: "zeta", user: "root" },
        { hostAlias: "Alpha", user: "alice" },
        { hostAlias: "alpha", user: "override" },
      ]),
    ).toEqual([
      { hostAlias: "Alpha", user: "alice" },
      { hostAlias: "alpha", user: "override" },
      { hostAlias: "zeta", user: "root" },
    ]);
  });
});

describe("resolveSshConfigPresetSelection", () => {
  const presets = [
    { hostAlias: "prod", user: "root", port: 2222, identityFile: "~/.ssh/prod" },
  ];

  test("keeps manual mode unchanged when manual alias is selected", () => {
    expect(resolveSshConfigPresetSelection(SSH_CONFIG_MANUAL_ALIAS, presets)).toEqual({
      selectedSshConfigAlias: SSH_CONFIG_MANUAL_ALIAS,
    });
  });

  test("falls back to manual mode when preset is missing", () => {
    expect(resolveSshConfigPresetSelection("missing", presets)).toEqual({
      selectedSshConfigAlias: SSH_CONFIG_MANUAL_ALIAS,
    });
  });

  test("applies preset fields when a known alias is selected", () => {
    expect(resolveSshConfigPresetSelection("prod", presets)).toEqual({
      selectedSshConfigAlias: "prod",
      host: "prod",
      username: "root",
      port: "2222",
      keyPath: "~/.ssh/prod",
      password: "",
      passphrase: "",
      authMethod: "ssh_config",
      label: "prod",
    });
  });
});

describe("applySshConfigSuggestionToForm", () => {
  test("only updates the selected alias for manual mode", () => {
    const calls: string[] = [];
    applySshConfigSuggestionToForm(SSH_CONFIG_MANUAL_ALIAS, [], {
      setSelectedSshConfigAlias: (value) => calls.push(`selected:${value}`),
      setHost: (value) => calls.push(`host:${value}`),
      setUsername: (value) => calls.push(`user:${value}`),
      setPort: (value) => calls.push(`port:${value}`),
      setKeyPath: (value) => calls.push(`key:${value}`),
      setPassword: (value) => calls.push(`password:${value}`),
      setPassphrase: (value) => calls.push(`passphrase:${value}`),
      setAuthMethod: (value) => calls.push(`auth:${value}`),
      setLabel: (value) => calls.push(`label:${value}`),
    });

    expect(calls).toEqual([`selected:${SSH_CONFIG_MANUAL_ALIAS}`]);
  });

  test("applies preset values through the provided setters", () => {
    const calls: string[] = [];
    applySshConfigSuggestionToForm("prod", [
      { hostAlias: "prod", user: "root", port: 2222, identityFile: "~/.ssh/prod" },
    ], {
      setSelectedSshConfigAlias: (value) => calls.push(`selected:${value}`),
      setHost: (value) => calls.push(`host:${value}`),
      setUsername: (value) => calls.push(`user:${value}`),
      setPort: (value) => calls.push(`port:${value}`),
      setKeyPath: (value) => calls.push(`key:${value}`),
      setPassword: (value) => calls.push(`password:${value}`),
      setPassphrase: (value) => calls.push(`passphrase:${value}`),
      setAuthMethod: (value) => calls.push(`auth:${value}`),
      setLabel: (value) => calls.push(`label:${value}`),
    });

    expect(calls).toEqual([
      "selected:prod",
      "host:prod",
      "user:root",
      "port:2222",
      "key:~/.ssh/prod",
      "password:",
      "passphrase:",
      "auth:ssh_config",
      "label:prod",
    ]);
  });
});

describe("buildSshFormSubmission", () => {
  test("returns null when host is blank", () => {
    expect(
      buildSshFormSubmission({
        host: "   ",
        port: "22",
        username: "root",
        authMethod: "key",
        keyPath: "~/.ssh/id_ed25519",
        password: "",
        passphrase: "",
        label: "",
      }),
    ).toBeNull();
  });

  test("requires a password when password auth is selected", () => {
    expect(
      buildSshFormSubmission({
        host: "server",
        port: "22",
        username: "root",
        authMethod: "password",
        keyPath: "",
        password: "",
        passphrase: "",
        label: "",
      }),
    ).toBeNull();
  });

  test("builds a key-auth payload with trimmed values and passphrase", () => {
    expect(
      buildSshFormSubmission({
        host: " server.example.com ",
        port: "not-a-number",
        username: " root ",
        authMethod: "key",
        keyPath: " ~/.ssh/id_ed25519 ",
        password: "ignored",
        passphrase: " secret ",
        label: " ",
      }),
    ).toEqual({
      id: "",
      label: "server.example.com",
      host: "server.example.com",
      port: 22,
      username: "root",
      authMethod: "key",
      keyPath: "~/.ssh/id_ed25519",
      password: undefined,
      passphrase: " secret ",
    });
  });

  test("builds a password-auth payload without key-path or passphrase", () => {
    expect(
      buildSshFormSubmission({
        host: "server",
        port: "2200",
        username: "root",
        authMethod: "password",
        keyPath: "~/.ssh/id_ed25519",
        password: "pw",
        passphrase: "secret",
        label: "My Server",
      }),
    ).toEqual({
      id: "",
      label: "My Server",
      host: "server",
      port: 2200,
      username: "root",
      authMethod: "password",
      keyPath: undefined,
      password: "pw",
      passphrase: undefined,
    });
  });
});

describe("submitSshForm", () => {
  test("returns false without calling onSubmit when the payload is invalid", () => {
    const calls: unknown[] = [];
    expect(
      submitSshForm({
        invokeId: "connect-ssh-form",
        host: " ",
        port: "22",
        username: "root",
        authMethod: "key",
        keyPath: "~/.ssh/id_ed25519",
        password: "",
        passphrase: "",
        label: "",
        onSubmit: (...args) => calls.push(args),
      }),
    ).toBe(false);
    expect(calls).toHaveLength(0);
  });

  test("calls onSubmit with the built payload when the form is valid", () => {
    const calls: unknown[] = [];
    expect(
      submitSshForm({
        invokeId: "connect-ssh-form",
        host: "server",
        port: "22",
        username: "root",
        authMethod: "key",
        keyPath: "~/.ssh/id_ed25519",
        password: "",
        passphrase: "",
        label: "My Server",
        onSubmit: (...args) => calls.push(args),
      }),
    ).toBe(true);
    expect(calls).toEqual([[
      "connect-ssh-form",
      {
        id: "",
        label: "My Server",
        host: "server",
        port: 22,
        username: "root",
        authMethod: "key",
        keyPath: "~/.ssh/id_ed25519",
        password: undefined,
        passphrase: undefined,
      },
    ]]);
  });
});

describe("formatSshConfigSuggestionLabel", () => {
  test("renders optional host metadata inline", () => {
    expect(
      formatSshConfigSuggestionLabel({
        hostAlias: "prod",
        hostName: "10.0.0.8",
        user: "root",
        port: 2222,
      }),
    ).toBe("prod (10.0.0.8) • root:2222");
  });

  test("omits the default port and missing fields", () => {
    expect(
      formatSshConfigSuggestionLabel({
        hostAlias: "prod",
        port: 22,
      }),
    ).toBe("prod");
  });
});
