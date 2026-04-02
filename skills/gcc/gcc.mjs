import { query } from "@anthropic-ai/claude-agent-sdk";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { parseArgs } from "node:util";

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    resume: { type: "string" },
  },
});

const dir = resolve(positionals[0] || ".");
const promptFile = positionals[1];
const resumeSessionId = values.resume;

// GLM API キー読み込み
const home = process.env.HOME || process.env.USERPROFILE;
const keyFile = resolve(home, ".claude/.glm-api-key");
if (!existsSync(keyFile)) {
  console.error(`[error] GLM API key not found: ${keyFile}`);
  process.exit(1);
}
const apiKey = readFileSync(keyFile, "utf8").trim();

// プロンプト読み込み
const prompt = promptFile ? readFileSync(promptFile, "utf8") : "";

const summaryPath = resolve(dir, ".gcc-summary.md");
const logPath = resolve(dir, ".gcc-agent.log");

// env オプションで子プロセスの環境変数を隔離（process.env を汚染しない）
// NOTE: ANTHROPIC_BASE_URL による Z.ai リダイレクトは非公式。SDK更新で壊れる可能性あり
const childEnv = { ...process.env };
// 親プロセスのプロバイダ変数を除去（Bedrock/Vertex 等の干渉防止）
for (const key of [
  "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY",
  "AWS_REGION", "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
]) {
  delete childEnv[key];
}
childEnv.ANTHROPIC_API_KEY = apiKey;
childEnv.ANTHROPIC_BASE_URL = "https://api.z.ai/api/anthropic";
childEnv.API_TIMEOUT_MS = "3000000";

const queryOpts = {
  prompt,
  options: {
    cwd: dir,
    model: "glm-5.1",
    permissionMode: "bypassPermissions",
    allowDangerouslySkipPermissions: true,
    settingSources: ["project", "user"],
    env: childEnv,
    ...(resumeSessionId ? { resume: resumeSessionId } : {}),
  },
};

let sessionId = null;
const logLines = [];

// 常に JSON を stdout に出力するため try/catch で包む（stderr は SDK の内部ログ用）
try {
  for await (const message of query(queryOpts)) {
    // セッションID取得（init メッセージから）
    if (message.type === "system" && message.subtype === "init" && message.session_id) {
      sessionId = message.session_id;
    }
    // ログ蓄積
    logLines.push(JSON.stringify(message));
  }

  // ログ書き出し
  writeFileSync(logPath, logLines.join("\n"), "utf8");

  // 結果出力（JSON: CCCがパース可能）
  console.log(JSON.stringify({
    sessionId,
    summaryExists: existsSync(summaryPath),
    summary: existsSync(summaryPath) ? readFileSync(summaryPath, "utf8") : null,
  }));
} catch (err) {
  // クラッシュ時もJSONで返す（stderr混入を防ぐ）
  writeFileSync(logPath, logLines.join("\n") + "\n[error] " + err.message, "utf8");
  console.log(JSON.stringify({ sessionId, error: err.message, summary: null }));
  process.exit(1);
}
