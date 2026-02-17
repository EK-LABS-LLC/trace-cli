FROM rust:1.86-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM node:22-bookworm-slim

# Install curl + jq for the test script
RUN apt-get update && apt-get install -y --no-install-recommends curl jq && rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI globally
RUN npm install -g @anthropic-ai/claude-code

# Copy pulse binary from builder
COPY --from=builder /app/target/release/pulse /usr/local/bin/pulse

COPY e2e/run.sh /e2e/run.sh
RUN chmod +x /e2e/run.sh

ENTRYPOINT ["/e2e/run.sh"]
