import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import type { SshHost } from "@/lib/types";

interface InstanceTabBarProps {
  hosts: SshHost[];
  activeId: string; // "local" or host.id
  connectionStatus: Record<string, "connected" | "disconnected" | "error">;
  onSelect: (id: string) => void;
  onHostsChange: () => void;
}

const emptyHost: Omit<SshHost, "id"> = {
  label: "",
  host: "",
  port: 22,
  username: "",
  authMethod: "ssh_config",
  keyPath: undefined,
  password: undefined,
};

export function InstanceTabBar({
  hosts,
  activeId,
  connectionStatus,
  onSelect,
  onHostsChange,
}: InstanceTabBarProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingHost, setEditingHost] = useState<SshHost | null>(null);
  const [form, setForm] = useState<Omit<SshHost, "id">>(emptyHost);
  const [saving, setSaving] = useState(false);

  const openAddDialog = () => {
    setEditingHost(null);
    setForm({ ...emptyHost });
    setDialogOpen(true);
  };

  const openEditDialog = (host: SshHost) => {
    setEditingHost(host);
    setForm({
      label: host.label,
      host: host.host,
      port: host.port,
      username: host.username,
      authMethod: host.authMethod,
      keyPath: host.keyPath,
      password: host.password,
    });
    setDialogOpen(true);
  };

  const handleSave = () => {
    const host: SshHost = {
      id: editingHost?.id ?? crypto.randomUUID(),
      ...form,
    };
    setSaving(true);
    api
      .upsertSshHost(host)
      .then(() => {
        onHostsChange();
        setDialogOpen(false);
      })
      .catch((e) => console.error("Failed to save SSH host:", e))
      .finally(() => setSaving(false));
  };

  const handleDelete = (hostId: string) => {
    api
      .deleteSshHost(hostId)
      .then(() => {
        onHostsChange();
        if (activeId === hostId) onSelect("local");
      })
      .catch((e) => console.error("Failed to delete SSH host:", e));
  };

  const statusDot = (status: "connected" | "disconnected" | "error" | undefined) => {
    const color =
      status === "connected"
        ? "bg-green-500"
        : status === "error"
          ? "bg-red-500"
          : "bg-gray-400";
    return <span className={cn("inline-block w-2 h-2 rounded-full shrink-0", color)} />;
  };

  return (
    <>
      <div className="flex items-center gap-0.5 px-2 py-1.5 bg-muted border-b border-border overflow-x-auto shrink-0">
        {/* Local tab */}
        <button
          className={cn(
            "flex items-center gap-1.5 px-3 py-1 rounded text-sm whitespace-nowrap transition-colors",
            activeId === "local"
              ? "bg-background shadow-sm font-medium"
              : "hover:bg-background/50"
          )}
          onClick={() => onSelect("local")}
        >
          {statusDot("connected")}
          Local
        </button>

        {/* Remote tabs */}
        {hosts.map((host) => (
          <div
            key={host.id}
            className="relative group flex items-center"
          >
            <button
              className={cn(
                "flex items-center gap-1.5 px-3 py-1 rounded text-sm whitespace-nowrap transition-colors",
                activeId === host.id
                  ? "bg-background shadow-sm font-medium"
                  : "hover:bg-background/50"
              )}
              onClick={() => onSelect(host.id)}
              onContextMenu={(e) => {
                e.preventDefault();
                openEditDialog(host);
              }}
            >
              {statusDot(connectionStatus[host.id])}
              {host.label || host.host}
            </button>
            <button
              className="absolute -top-0.5 -right-0.5 hidden group-hover:flex items-center justify-center w-4 h-4 rounded-full bg-muted-foreground/20 hover:bg-destructive hover:text-white text-[10px] leading-none"
              onClick={(e) => {
                e.stopPropagation();
                handleDelete(host.id);
              }}
            >
              &times;
            </button>
          </div>
        ))}

        {/* Add button */}
        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-2 shrink-0 text-xs"
          onClick={openAddDialog}
        >
          + SSH
        </Button>
      </div>

      {/* Add/Edit Dialog */}
      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {editingHost ? "Edit Remote Instance" : "Add Remote Instance"}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="ssh-label">Label</Label>
              <Input
                id="ssh-label"
                value={form.label}
                onChange={(e) => setForm((f) => ({ ...f, label: e.target.value }))}
                placeholder="My Server"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="ssh-host">Host</Label>
              <Input
                id="ssh-host"
                value={form.host}
                onChange={(e) => setForm((f) => ({ ...f, host: e.target.value }))}
                placeholder="192.168.1.100"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="ssh-port">Port</Label>
              <Input
                id="ssh-port"
                type="number"
                value={form.port}
                onChange={(e) =>
                  setForm((f) => ({ ...f, port: parseInt(e.target.value, 10) || 22 }))
                }
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="ssh-username">Username</Label>
              <Input
                id="ssh-username"
                value={form.username}
                onChange={(e) => setForm((f) => ({ ...f, username: e.target.value }))}
                placeholder="(optional, defaults to current user)"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
              />
            </div>
            <div className="space-y-1.5">
              <Label>Auth Method</Label>
              <Select
                value={form.authMethod}
                onValueChange={(val) =>
                  setForm((f) => ({
                    ...f,
                    authMethod: val as SshHost["authMethod"],
                    keyPath: val === "key" ? (f.authMethod === "key" ? f.keyPath : "") : undefined,
                    password: val === "password" ? (f.authMethod === "password" ? f.password : "") : undefined,
                  }))
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="ssh_config">SSH Config / Agent</SelectItem>
                  <SelectItem value="key">Private Key</SelectItem>
                  <SelectItem value="password">Password</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {form.authMethod === "key" && (
              <div className="space-y-1.5">
                <Label htmlFor="ssh-keypath">Key Path</Label>
                <Input
                  id="ssh-keypath"
                  value={form.keyPath || ""}
                  onChange={(e) => setForm((f) => ({ ...f, keyPath: e.target.value }))}
                  placeholder="~/.ssh/id_rsa"
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck={false}
                />
              </div>
            )}
            {form.authMethod === "password" && (
              <div className="space-y-1.5">
                <Label htmlFor="ssh-password">Password</Label>
                <Input
                  id="ssh-password"
                  type="password"
                  value={form.password || ""}
                  onChange={(e) => setForm((f) => ({ ...f, password: e.target.value }))}
                />
                <p className="text-xs text-yellow-600 dark:text-yellow-400">
                  Password auth requires sshpass on the system. Recommended: use SSH Config / Agent mode instead.
                </p>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialogOpen(false)} disabled={saving}>
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={saving || !form.host}>
              {saving ? "Saving..." : editingHost ? "Update" : "Add"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
