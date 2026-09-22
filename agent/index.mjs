#!/usr/bin/env node
import { createInterface } from "node:readline";

const API_BASE = process.env.HUIMENG_API_BASE || "https://tapxflow.com";
const API_TOKEN = process.env.HUIMENG_TOKEN || "";

async function callCloud(path, body, method) {
  if (!API_TOKEN) throw new Error("未配置 HUIMENG_TOKEN 环境变量（值为 hm_sk_ 开头的 API Key）");
  const res = await fetch(API_BASE + path, {
    method: method || (body ? "POST" : "GET"),
    headers: {
      "Authorization": "Bearer " + API_TOKEN,
      "Content-Type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  if (!res.ok) throw new Error("云端请求失败: " + res.status + " " + text.slice(0, 500));
  return text;
}

const TOOLS = [
  {
    name: "list_projects",
    description: "列出当前账号下的短剧项目",
    inputSchema: {
      type: "object",
      properties: {
        page: { type: "integer", description: "页码，默认 1" },
        page_size: { type: "integer", description: "每页数量，默认 20" },
      },
      additionalProperties: false,
    },
  },
  {
    name: "create_project",
    description: "创建短剧项目",
    inputSchema: {
      type: "object",
      properties: {
        title: { type: "string", description: "项目标题" },
        description: { type: "string", description: "项目简介" },
      },
      required: ["title"],
      additionalProperties: false,
    },
  },
  {
    name: "list_episodes",
    description: "列出指定项目的剧集列表",
    inputSchema: {
      type: "object",
      properties: {
        project_id: { type: "integer", description: "项目 ID" },
      },
      required: ["project_id"],
      additionalProperties: false,
    },
  },
  {
    name: "create_episode",
    description: "在项目下创建剧集并提交剧本文本",
    inputSchema: {
      type: "object",
      properties: {
        project_id: { type: "integer", description: "项目 ID" },
        title: { type: "string", description: "剧集标题" },
        script: { type: "string", description: "剧本正文" },
      },
      required: ["project_id", "title", "script"],
      additionalProperties: false,
    },
  },
  {
    name: "generate_storyboards",
    description: "为剧集生成分镜板（消耗积分，需用户确认）",
    inputSchema: {
      type: "object",
      properties: {
        episode_id: { type: "integer", description: "剧集 ID" },
      },
      required: ["episode_id"],
      additionalProperties: false,
    },
  },
  {
    name: "generate_video",
    description: "发起视频生成（消耗积分，需用户确认）",
    inputSchema: {
      type: "object",
      properties: {
        prompt: { type: "string", description: "生成提示词" },
        model: { type: "string", description: "模型名，可用 list_models 查询" },
        duration: { type: "integer", description: "时长秒数，默认 5" },
        ratio: { type: "string", description: "画面比例，默认 16:9" },
        image_url: { type: "string", description: "首帧参考图 URL（可选）" },
      },
      required: ["prompt"],
      additionalProperties: false,
    },
  },
  {
    name: "generate_image",
    description: "发起图片生成（消耗积分，需用户确认）",
    inputSchema: {
      type: "object",
      properties: {
        prompt: { type: "string", description: "生成提示词" },
        model: { type: "string", description: "模型名，可用 list_models 查询" },
        width: { type: "integer", description: "宽度，默认 1024" },
        height: { type: "integer", description: "高度，默认 1024" },
        reference_urls: { type: "array", items: { type: "string" }, description: "参考图 URL 列表（可选）" },
      },
      required: ["prompt"],
      additionalProperties: false,
    },
  },
  {
    name: "get_task_status",
    description: "查询生成任务状态与结果",
    inputSchema: {
      type: "object",
      properties: {
        task_id: { type: "string", description: "任务 ID" },
      },
      required: ["task_id"],
      additionalProperties: false,
    },
  },
  {
    name: "list_models",
    description: "列出可用的图片与视频生成模型",
    inputSchema: {
      type: "object",
      properties: {
        type: { type: "string", description: "筛选类型: image 或 video（可选）" },
      },
      additionalProperties: false,
    },
  },
];

async function dispatch(name, args) {
  switch (name) {
    case "list_projects": {
      const q = new URLSearchParams();
      if (args.page) q.set("page", String(args.page));
      if (args.page_size) q.set("page_size", String(args.page_size));
      const qs = q.toString();
      return callCloud("/api/v1/open/drama/projects" + (qs ? "?" + qs : ""));
    }
    case "create_project":
      return callCloud("/api/v1/open/drama/projects", args);
    case "list_episodes":
      return callCloud("/api/v1/open/drama/projects/" + args.project_id + "/episodes");
    case "create_episode":
      return callCloud("/api/v1/open/drama/episodes", args);
    case "generate_storyboards":
      return callCloud("/api/v1/open/drama/episodes/" + args.episode_id + "/generate-storyboards", {});
    case "generate_video":
      return callCloud("/api/v1/open/generate/video", args);
    case "generate_image":
      return callCloud("/api/v1/open/generate/image", args);
    case "get_task_status":
      return callCloud("/api/v1/open/generate/query/" + encodeURIComponent(args.task_id));
    case "list_models": {
      const q = args.type ? "?type=" + encodeURIComponent(args.type) : "";
      return callCloud("/api/v1/open/generate/modes" + q);
    }
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
        serverInfo: { name: "huimeng-agent", version: "0.2.0" },
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
