import { spawn } from "child_process";

const SOURCE = "openclaw";

function emitSpan(eventType: string, payload: Record<string, unknown>): void {
  const proc = spawn("pulse", ["emit", eventType], {
    stdio: ["pipe", "ignore", "ignore"],
  });
  proc.stdin!.write(JSON.stringify({ ...payload, source: SOURCE }));
  proc.stdin!.end();
}

interface OpenClawEvent {
  type: string;
  action: string;
  sessionKey: string;
  timestamp: Date;
  messages: string[];
  context: {
    cfg?: {
      agents?: {
        defaults?: {
          model?: {
            primary?: string;
          };
          workspace?: string;
        };
      };
    };
    workspaceDir?: string;
    // message:received context
    from?: string;
    content?: string;
    channelId?: string;
    // message:sent context
    to?: string;
    success?: boolean;
    [key: string]: unknown;
  };
}

export default async function handler(event: OpenClawEvent): Promise<void> {
  const sessionId = event.sessionKey;
  if (!sessionId) return;

  const cwd =
    event.context?.workspaceDir ??
    event.context?.cfg?.agents?.defaults?.workspace;
  const base = { session_id: sessionId, cwd };
  const eventKey = `${event.type}:${event.action}`;

  switch (eventKey) {
    case "command:new": {
      const model = event.context?.cfg?.agents?.defaults?.model?.primary;
      emitSpan("session_start", { ...base, model });
      break;
    }
    case "command:stop":
      emitSpan("stop", base);
      break;
    case "command:reset":
      emitSpan("session_end", { ...base, reason: "reset" });
      break;
    case "message:received":
      emitSpan("user_prompt_submit", {
        ...base,
        prompt: event.context?.content,
      });
      break;
    case "message:sent":
      emitSpan("notification", {
        ...base,
        message: event.context?.content,
      });
      break;
  }
}
