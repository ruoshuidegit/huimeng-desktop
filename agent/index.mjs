#!/usr/bin/env node
import { createInterface } from "node:readline";

const API_BASE = process.env.HUIMENG_API_BASE || "https://tapxflow.com";
const API_TOKEN = process.env.HUIMENG_TOKEN || "";

async function callCloud(path, body) {
  if (!API_TOKEN) throw new Error("未配置 HUIMENG_TOKEN 环境变量");
  const res = await fetch(API_BASE + path, {
    method: body ? "POST" : "GET",
    headers: {
      "Authorization": "Bearer " + API_TOKEN,
      "Content-Type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  if (!res.ok) throw new Error("云端请求失败: " + res.status);
  return text;
}

const TOOLS = [
  {
    name: "list_projects",
    description: "列出当前账号下的画布项目",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "get_task_status",
    description: "查询生成任务状态",
    inputSchema: {
      type: "object",
      properties: { task_id: { type: "string", description: "任务 ID" } },
      required: ["task_id"],
      additionalProperties: false,
    },
  },
  {
    name: "create_shot",
    description: "在指定项目中创建分镜",
    inputSchema: {
      type: "object",
      properties: {
        project_id: { type: "string", description: "项目 ID" },
        script: { type: "string", description: "剧本段落" },
        style: { type: "string", description: "风格标签" },
      },
      required: ["project_id", "script"],
      additionalProperties: false,
    },
  },
  {
    name: "generate_video",
    description: "发起视频生成（消耗积分，需用户确认）",
    inputSchema: {
      type: "object",
      properties: {
        shot_id: { type: "string", description: "分镜 ID" },
        mode: { type: "string", description: "生成模式" },
      },
      required: ["shot_id"],
      additionalProperties: false,
    },
  },
];

async function dispatch(name, args) {
  switch (name) {
    case "list_projects":
      return callCloud("/api/v1/agent/projects");
    case "get_task_status":
      return callCloud("/api/v1/agent/tasks/" + encodeURIComponent(args.task_id));
    case "create_shot":
      return callCloud("/api/v1/agent/shots", args);
    case "generate_video":
      return callCloud("/api/v1/agent/generate", args);
    default:
      throw new Error("未知工具: " + name);
  }
}

const rl = createInterface({ input: process.stdin });
rl.on("line", async (line) => {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }
  const reply = { jsonrpc: "2.0", id: msg.id };
  try {
    if (msg.method === "initialize") {
      reply.result = {
        protocolVersion: "2024-11-05",
        capabilities: { tools: {} },
        serverInfo: { name: "huimeng-agent", version: "0.1.0" },
      };
    } else if (msg.method === "tools/list") {
      reply.result = { tools: TOOLS };
    } else if (msg.method === "tools/call") {
      const out = await dispatch(msg.params.name, msg.params.arguments || {});
      reply.result = { content: [{ type: "text", text: String(out) }] };
    } else if (msg.method === "notifications/initialized") {
      return;
    } else {
      reply.error = { code: -32601, message: "Method not found" };
    }
  } catch (err) {
    reply.error = { code: -32000, message: err.message };
  }
  process.stdout.write(JSON.stringify(reply) + "\n");
});
