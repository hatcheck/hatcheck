// Simplified Chinese localization, with English as the source and fallback. No runtime dependency.
export type Locale = "en" | "zh-CN";
type Preference = "system" | Locale;

const KEY = "hatcheck.locale";
const ZH: Record<string, string> = {
  "Sessions": "会话", "Workspaces": "工作区", "Plans": "用量", "Profiles": "配置", "Desktop": "桌面", "Settings": "设置",
  "Refresh": "刷新", "Open command palette": "打开命令面板", "Refresh sessions and plans": "刷新会话和用量",
  "Search titles and folders": "搜索标题和文件夹", "Search titles, aliases, tags and folders": "搜索标题、别名、标签和文件夹",
  "Every agent": "全部智能体", "Every profile": "全部配置", "Last 24 hours": "最近 24 小时", "Last 3 days": "最近 3 天",
  "Last week": "最近一周", "Last 30 days": "最近 30 天", "Desktop, SDK, Codex app": "桌面、SDK、Codex 应用",
  "Model": "模型", "Any cost": "全部费用", "Under $1": "低于 $1", "$1–$10": "$1–$10", "$10 or more": "$10 或更多",
  "Every tag": "全部标签", "Favorites": "收藏", "Include archived": "显示已归档", "Saved views…": "已保存的视图…",
  "Save view": "保存视图", "Remove view": "删除视图", "Export shown…": "导出筛选结果…", "Loading…": "加载中…",
  "Pick a session to see its conversation.": "选择一个会话以查看对话。", "No sessions match.": "没有匹配的会话。",
  "Pinned project folders, their bound profiles, recent sessions and cached usage. A new session starts only in the agent and profile you explicitly choose.": "已固定的项目文件夹、绑定的配置、最近会话和缓存用量。新会话只会在你明确选择的智能体和配置中启动。",
  "Project folder, such as ~/code/api": "项目文件夹，例如 ~/code/api", "Display name (optional)": "显示名称（可选）", "Pin workspace": "固定工作区",
  "No pinned workspaces. Add a project folder above.": "还没有固定的工作区。请在上方添加项目文件夹。",
  "No folder bindings.": "没有文件夹绑定。", "No sessions in the selected time range.": "所选时间范围内没有会话。", "New session…": "新会话…", "Unpin": "取消固定",
  "What's left of each plan, sorted as hatcheck usage sorts it: accounts with Fable left first, the soonest Fable reset on top. The second bar is the time left until the limit resets.": "显示各套餐的剩余额度，并按 hatcheck usage 的顺序排列：有 Fable 剩余额度的账号优先，其中最快重置的排在前面。第二条进度条表示距离额度重置的时间。",
  "Each agent profile or home has its own login. Signing in happens in the agent, in a terminal (or DSH's web UI); Hatcheck never sees a password or token.": "每个智能体配置或主目录都有独立登录。登录在智能体的终端（或 DSH 网页界面）中完成；Hatcheck 不会看到密码或令牌。",
  "Add one": "添加配置", "Sign in, in a terminal": "在终端中登录", "Profile or home": "配置或主目录", "Agent": "智能体", "Plan": "套餐", "Shares an account with": "共享账号的配置",
  "Sign in…": "登录…", "No profiles or homes yet.": "还没有配置或主目录。",
  "Claude Desktop instances, each with its own data folder and login. Opening one brings it forward if it's running.": "Claude 桌面实例，每个都有独立的数据文件夹和登录。打开正在运行的实例会将其窗口置于前台。",
  "Make one": "创建桌面实例", "Make it": "创建", "Colour": "颜色", "Bring forward": "置于前台", "Open": "打开", "No instances yet.": "还没有桌面实例。",
  "Ask for live usage. Off: show what Claude Code and Codex last saved. On: run Claude Code's /usage in each profile, which asks Anthropic with that profile's own login, and ask ChatGPT's usage endpoint for each Codex home with its saved login, read and never refreshed; they rate-limit, so not too often.": "请求实时用量。关闭：显示 Claude Code 和 Codex 最近保存的数据。开启：在每个配置中运行 Claude Code 的 /usage，使用该配置自己的登录向 Anthropic 查询，并使用 Codex 主目录保存的登录向 ChatGPT 用量端点查询；令牌只读取、绝不刷新。这些接口有限流，请勿频繁查询。",
  "Refresh every": "刷新间隔", "minutes": "分钟", "Notify me when a limit runs low, when it resets, and when one is about to reset with much of it unused": "在额度不足、额度重置，以及额度即将重置但仍有大量未用时通知我",
  "Low means": "低额度阈值", "% or less left": "% 或更少剩余", "Much unused means": "未使用过多阈值", "% or more left as the reset nears (0: never)": "% 或更多剩余且接近重置（0：从不通知）",
  "Terminal for Resume": "用于继续会话的终端", "Usual one": "系统默认", "Check for updates": "检查更新", "About": "关于",
  "Unofficial: not made by, affiliated with or endorsed by Anthropic, OpenAI, Google, Moonshot AI, Z.ai, DeepSeek, MiniMax, Alibaba Cloud, GitHub or the OpenCode project. Claude, Claude Code and Claude Desktop are trademarks of Anthropic; Codex and ChatGPT are trademarks of OpenAI; Antigravity and Gemini are trademarks of Google LLC; Kimi is Moonshot AI's, ZCode and GLM are Z.ai's, DeepSeek Harness is DeepSeek's, MiniMax and MiniMax Code are MiniMax's, Qwen and Qwen Code are Alibaba Cloud's, GitHub Copilot is GitHub's, and OpenCode is the OpenCode project's.": "非官方：并非由 Anthropic、OpenAI、Google、Moonshot AI、Z.ai、DeepSeek、MiniMax、阿里云、GitHub 或 OpenCode 项目开发、附属或认可。Claude、Claude Code 和 Claude Desktop 是 Anthropic 的商标；Codex 和 ChatGPT 是 OpenAI 的商标；Antigravity 和 Gemini 是 Google LLC 的商标；Kimi 是 Moonshot AI 的商标；ZCode 和 GLM 是 Z.ai 的商标；DeepSeek Harness 是 DeepSeek 的商标；MiniMax 和 MiniMax Code 是 MiniMax 的商标；Qwen 和 Qwen Code 是阿里云的商标；GitHub Copilot 是 GitHub 的商标；OpenCode 是 OpenCode 项目的商标。",
  "It shows what each plan has left and helps you pick an account you may use for the work at hand; it never switches accounts by itself.": "它显示各套餐的剩余额度，帮助你选择可用于当前工作的账号；绝不会自行切换账号。",
  "Resume in a terminal": "在终端中继续", "Open DeepSeek Harness": "打开 DeepSeek Harness", "Copy command": "复制命令", "Move to another profile…": "移动到其他配置…",
  "Favorite": "收藏", "Unfavorite": "取消收藏", "Edit labels…": "编辑标签…", "Archive": "归档", "Unarchive": "取消归档", "Export…": "导出…",
  "Cancel": "取消", "Preview": "预览", "Move it": "移动", "Close": "关闭", "Resume it there": "在那里继续", "Undo move": "撤销移动",
  "Session labels": "会话标签", "Personal alias": "个人别名", "Original agent title": "智能体原始标题", "Tags": "标签", "backend, urgent": "后端，紧急", "Save labels": "保存标签",
  "Save this session view": "保存此会话视图", "Name": "名称", "Such as Backend favorites": "例如：后端收藏", "Remove filter": "删除筛选视图",
  "Format": "格式", "Destination folder": "目标文件夹", "Replace existing files": "替换已有文件", "Export": "导出", "Export complete": "导出完成", "Export failed": "导出失败",
  "From": "来源", "To": "目标", "Preserved:": "已保留：", "Preserved as text:": "以文本保留：", "Left out:": "未包含：", "Same session": "同一会话", "New session": "新会话",
  "Start new session": "开始新会话", "Agent and profile": "智能体和配置", "folder binding": "文件夹绑定", "No saved view": "没有已保存的视图",
  "Show usage and local insights": "显示用量和本地分析", "Browse sessions": "浏览会话", "Browse workspaces": "浏览工作区", "Browse profiles": "浏览配置",
  "Resume a session or run a command…": "继续会话或运行命令…", "Search sessions and commands": "搜索会话和命令", "Matching sessions and commands": "匹配的会话和命令",
  "↑↓ choose · Enter run · Esc close": "↑↓ 选择 · Enter 运行 · Esc 关闭", "No matching sessions or commands.": "没有匹配的会话或命令。",
  "Likely to run out": "可能提前用尽", "Likely to go unused": "可能用不完", "Projects": "项目", "Models": "模型",
  "No plan readings in this period. Hatcheck records them as it reads plans; no reading isn't zero usage.": "此时间段没有套餐读数。Hatcheck 会在读取套餐时记录；没有读数不等于用量为零。",
  "readings": "次读取", "sessions": "个会话", "with recorded costs": "个有费用记录", "files": "个文件", "left": "剩余", "resets": "重置时间",
  "Unknown": "未知", "no limit data": "没有额度数据", "live": "实时", "cached": "缓存", "no data": "无数据", "open": "运行中", "bound to": "绑定到",
  "CLI": "命令行", "hour": "小时", "hours": "小时", "day": "天", "days": "天", "week": "周", "month": "月",
  "Copied:": "已复制：", "Done:": "已完成：", "Workspace pinned": "工作区已固定", "Workspace unpinned; its folder wasn't changed": "工作区已取消固定；文件夹未更改",
  "Session labels saved": "会话标签已保存", "Session archived; its transcript is unchanged": "会话已归档；其记录未更改", "Session unarchived": "会话已取消归档",
  "Move undone; any backup is kept": "移动已撤销；备份已保留", "Opening a terminal for the new session": "正在打开新会话的终端",
  "Made it": "已创建", "Updates are checked in the app.": "请在应用中检查更新。", "Checking…": "正在检查…", "This is the newest version.": "已是最新版本。",
  "Installed; restarting…": "已安装；正在重启…", "Couldn't check:": "无法检查：", "Unofficial": "非官方",
  "Recent sessions": "最近会话", "last 24h": "最近 24 小时", "Claude Desktop": "Claude 桌面", "Quit": "退出",
  "Open the Hatcheck window": "打开 Hatcheck 窗口", "sorted by Fable": "按 Fable 排序", "Resume": "继续", "Copy": "复制",
  "General": "通用", "System language": "跟随系统语言", "Language": "语言", "Simplified Chinese": "简体中文", "English": "English",
};

const RULES: [RegExp, (match: RegExpMatchArray) => string][] = [
  [/^Export (\d+) shown…$/, (m) => `导出 ${m[1]} 个筛选结果…`],
  [/^(\d+) sessions$/, (m) => `${m[1]} 个会话`],
  [/^(\d+) with recorded costs$/, (m) => `${m[1]} 个有费用记录`],
  [/^(\d+) readings$/, (m) => `${m[1]} 次读取`],
  [/^Local insights · (\d+) days$/, (m) => `本地用量分析 · 最近 ${m[1]} 天`],
  [/^Usage over time for (.+)$/, (m) => `${m[1]} 的用量趋势`],
  [/^Runs out (.+)$/, (m) => `预计用尽时间：${m[1]}`],
  [/^(\d+(?:\.\d+)?)% left · resets (.+)$/, (m) => `剩余 ${m[1]}% · 重置时间：${m[2]}`],
  [/^(\d+(?:\.\d+)?)% left$/, (m) => `剩余 ${m[1]}%`],
  [/^resets (.+)$/, (m) => `重置时间：${m[1]}`],
  [/^(\d+) documents$/, (m) => `${m[1]} 个文档`],
  [/^(\d+) document$/, (m) => `${m[1]} 个文档`],
  [/^And (\d+) more\.$/, (m) => `另有 ${m[1]} 个。`],
  [/^Export (\d+) session$/, (m) => `导出 ${m[1]} 个会话`],
  [/^Export (\d+) sessions$/, (m) => `导出 ${m[1]} 个会话`],
  [/^Resume “(.+)”$/, (m) => `继续“${m[1]}”`],
  [/^Move “(.+)” to another profile$/, (m) => `将“${m[1]}”移动到其他配置`],
  [/^Export “(.+)”$/, (m) => `导出“${m[1]}”`],
  [/^Open or sign in to (.+)$/, (m) => `打开或登录 ${m[1]}`],
  [/^New session in (.+)$/, (m) => `在 ${m[1]} 中开始新会话`],
  [/^Opening a terminal for (.+)$/, (m) => `正在为 ${m[1]} 打开终端`],
  [/^(\d+) earlier turns aren't shown\.$/, (m) => `较早的 ${m[1]} 轮未显示。`],
  [/^New session: (.+)$/, (m) => `新会话：${m[1]}`],
  [/^Saved view "(.+)"$/, (m) => `已保存视图“${m[1]}”`],
  [/^Removed view "(.+)"$/, (m) => `已删除视图“${m[1]}”`],
  [/^Version (.+) is out; downloading…$/, (m) => `版本 ${m[1]} 已发布；正在下载…`],
];

export function preference(): Preference {
  try {
    const value = localStorage.getItem(KEY);
    return value === "en" || value === "zh-CN" ? value : "system";
  } catch {
    return "system";
  }
}

export function locale(): Locale {
  if (preference() !== "system") return preference() as Locale;
  return typeof navigator !== "undefined" && navigator.languages.some((lang) => lang.toLowerCase().startsWith("zh")) ? "zh-CN" : "en";
}

export function translateText(text: string, language: Locale = locale()): string {
  if (language === "en") return text;
  const match = text.match(/^(\s*)([\s\S]*?)(\s*)$/);
  const lead = match?.[1] || "", body = match?.[2] || "", tail = match?.[3] || "";
  if (ZH[body]) return lead + ZH[body] + tail;
  for (const [pattern, translate] of RULES) {
    const found = body.match(pattern);
    if (found) return lead + translate(found) + tail;
  }
  return text;
}

const originalText = new WeakMap<Text, string>();
const originalAttributes = new WeakMap<Element, Map<string, string>>();

function translateNode(node: Node, language: Locale) {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = node as Text;
    const parent = text.parentElement;
    if (parent?.closest("code, pre, .mono, script, style")) return;
    const original = originalText.get(text) ?? text.nodeValue ?? "";
    originalText.set(text, original);
    text.nodeValue = translateText(original, language);
    return;
  }
  if (!(node instanceof Element)) return;
  const element = node as Element;
  for (const name of ["placeholder", "title", "aria-label"]) {
    if (!element.hasAttribute(name) || element.closest("code, pre, script, style")) continue;
    const attributes = originalAttributes.get(element) || new Map<string, string>();
    const original = attributes.get(name) ?? element.getAttribute(name) ?? "";
    attributes.set(name, original);
    originalAttributes.set(element, attributes);
    element.setAttribute(name, translateText(original, language));
  }
  for (const child of node.childNodes) translateNode(child, language);
}

export function applyLocale() {
  if (typeof document === "undefined") return;
  const language = locale();
  document.documentElement.lang = language;
  translateNode(document.body, language);
  const select = document.getElementById("set-language") as HTMLSelectElement | null;
  if (select) select.value = preference();
}

export function setLocalePreference(value: Preference) {
  try {
    localStorage.setItem(KEY, value);
  } catch {
    // A private window still gets the selected language for this session.
  }
  applyLocale();
}

let started = false;
export function startLocalization() {
  if (started || typeof document === "undefined") return;
  started = true;
  applyLocale();
  const observer = new MutationObserver((changes) => {
    for (const change of changes) for (const node of change.addedNodes) translateNode(node, locale());
  });
  observer.observe(document.body, { childList: true, subtree: true });
}
