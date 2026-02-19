import type { Plugin } from "@opencode-ai/plugin";

const SOURCE = "opencode";

function emitSpan(eventType: string, payload: Record<string, unknown>) {
  const proc = Bun.spawn(["pulse", "emit", eventType], {
    stdin: "pipe",
    stdout: "ignore",
    stderr: "ignore",
  });
  proc.stdin.write(JSON.stringify({ ...payload, source: SOURCE }));
  proc.stdin.end();
}

export const PulsePlugin: Plugin = async (ctx) => {
  const cwd = ctx.directory;

  const base = (sessionID: string) => ({ session_id: sessionID, cwd });

  return {
    event: async ({ event }) => {
      switch (event.type) {
        case "session.created":
          emitSpan(
            "session_start",
            base((event.properties as any).info?.id),
          );
          break;
        case "session.idle":
          emitSpan("session_end", {
            ...base((event.properties as any).sessionID),
            reason: "idle",
          });
          break;
        case "session.error":
          emitSpan("session_end", {
            ...base((event.properties as any).sessionID),
            reason: "error",
            error: (event.properties as any).error,
          });
          break;
        case "message.updated": {
          const info = (event.properties as any).info;
          if (info?.role === "user") {
            emitSpan("user_prompt_submit", {
              ...base(info.sessionID),
              prompt:
                typeof info.content === "string"
                  ? info.content
                  : JSON.stringify(info.content),
            });
          } else if (info?.role === "assistant") {
            emitSpan("assistant_message", {
              ...base(info.sessionID),
              model: info.modelID,
              tokens: info.tokens,
              cost: info.cost,
            });
          }
          break;
        }
      }
    },

    "tool.execute.before": async ({ tool, sessionID, callID }, { args }) => {
      emitSpan("pre_tool_use", {
        ...base(sessionID),
        tool_name: tool,
        tool_input: args,
        tool_use_id: callID,
      });
    },

    "tool.execute.after": async (
      { tool, sessionID, callID },
      { output, metadata },
    ) => {
      emitSpan("post_tool_use", {
        ...base(sessionID),
        tool_name: tool,
        tool_response: output,
        tool_use_id: callID,
      });
    },
  };
};
