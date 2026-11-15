//! Opt-in Simplified Chinese for human-facing CLI output and help. HATCHECK_LANG=zh-CN is the only
//! switch: no locale detection, and no change to JSON, raw output, commands, paths or version output.

#[derive(Clone, Copy, PartialEq)]
pub enum Locale {
    English,
    ZhCn,
}

pub fn locale() -> Locale {
    match std::env::var("HATCHECK_LANG")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "zh-cn" | "zh_cn" | "zh" => Locale::ZhCn,
        _ => Locale::English,
    }
}

fn exact(text: &str) -> Option<&'static str> {
    Some(match text {
        "Usage insights, last 7 days (local only)" => "本地用量分析：最近 7 天",
        "Deep checks:" => "深度检查：",
        "Storage by profile or home:" => "按配置或主目录统计的存储：",
        "Profiles:" => "配置：",
        "Projects:" => "项目：",
        "Models:" => "模型：",
        "No pinned workspaces yet. Add this folder with: hatcheck workspaces add" => {
            "还没有固定的工作区。请使用：hatcheck workspaces add"
        }
        "No plan readings in this period; hatcheck usage records them as it reads plans." => {
            "此时间段没有套餐读数。Hatcheck 会在读取套餐时记录；没有读数不等于用量为零。"
        }
        "No session, binding or launcher problems found." => "未发现会话、绑定或启动器问题。",
        "No sessions match this export" => "没有符合导出条件的会话",
        "No sessions match." => "没有匹配的会话。",
        "Cleared the session's Hatcheck metadata. Its transcript is unchanged." => {
            "已清除该会话的 Hatcheck 元数据；其记录未更改。"
        }
        "Backup saved in" => "备份已保存到",
        "Backup would be in" => "备份将保存到",
        "Backups are available for same-agent Claude Code, Codex and Kimi moves" => {
            "备份支持同智能体的 Claude Code、Codex 和 Kimi 移动"
        }
        "Nothing changed." => "没有更改。",
        "Nothing started." => "未启动任何内容。",
        "No saved view" => "没有已保存的视图",
        "No saved filter" => "没有已保存的筛选视图",
        _ => return None,
    })
}

fn dynamic(text: &str) -> String {
    if let Some(rest) = text.strip_prefix("Pinned workspace ") {
        if let Some((name, path)) = rest.split_once(" at ") {
            return format!("已固定工作区 {name}，位置：{path}");
        }
    }
    if let Some(rest) = text.strip_prefix("Unpinned workspace ") {
        if let Some((name, _)) = rest.split_once(". Its folder is still there.") {
            return format!("已取消固定工作区 {name}；文件夹仍然保留。");
        }
    }
    if let Some(name) = text.strip_prefix("Saved filter ") {
        return format!("已保存筛选视图 {name}");
    }
    if let Some(name) = text.strip_prefix("Removed filter ") {
        return format!("已删除筛选视图 {name}");
    }
    if let Some(id) = text.strip_prefix("Updated Hatcheck's library entry for ") {
        return format!("已更新 {id} 的 Hatcheck 会话库条目。");
    }
    if let Some(rest) = text.strip_prefix("Exported ") {
        if let Some((count, format)) = rest.split_once(" as ") {
            let (count, format) = (
                count
                    .replace("sessions", "个会话")
                    .replace("session", "个会话"),
                format,
            );
            return format!("已导出 {count}，格式：{format}");
        }
    }
    if let Some(path) = text.strip_prefix("Backup saved in ") {
        return format!("备份已保存到 {path}");
    }
    if let Some(path) = text.strip_prefix("Backup would be in ") {
        return format!("备份将保存到 {path}");
    }
    if let Some(path) = text.strip_prefix("No saved filter named ") {
        return format!("没有名为 {path} 的已保存筛选视图");
    }
    if let Some(path) = text.strip_prefix("Project folder ") {
        if let Some((path, _)) = path.split_once(" isn't there") {
            return format!("项目文件夹 {path} 不存在");
        }
    }
    if let Some(path) = text.strip_prefix("Workspace folder ") {
        if let Some((path, _)) = path.split_once(" isn't there") {
            return format!("工作区文件夹 {path} 不存在");
        }
    }
    if let Some(rest) = text.strip_prefix("Sessions: ") {
        if let Some((sessions, costs)) = rest.split_once(" · recorded costs: ") {
            return format!("会话：{sessions} · 有费用记录：{costs}");
        }
    }
    if let Some(rest) = text.strip_prefix("Usage insights, last ") {
        if let Some((days, _)) = rest.split_once(" days (local only)") {
            return format!("本地用量分析：最近 {days} 天");
        }
    }
    if let Some(id) = text.strip_prefix("No session starting with ") {
        return format!("没有找到以此 ID 开头的会话：{id}");
    }
    if text.starts_with("Starting a new ") {
        return text
            .replace("Starting a new ", "开始新的")
            .replace(" session with ", "会话，使用配置 ")
            .replace(" here, in ", "，在当前终端，目录：")
            .replace(" in a new ", "，在新 ")
            .replace(" window, in ", " 窗口中，目录：");
    }
    if text.contains(": runs out ") && text.contains(" at ") {
        return text
            .replace(": runs out ", "：预计用尽时间：")
            .replace(" at ", "，速度：");
    }
    if text.contains("% left, resets ") {
        return text.replace("% left, resets ", "% 剩余 · 重置时间：");
    }
    if text.contains(" is bound to ") && text.contains("; choose that profile instead of ") {
        return text
            .replace(" is bound to ", " 已绑定到 ")
            .replace(" for ", "，智能体：")
            .replace(
                "; choose that profile instead of ",
                "；请改用该配置，而不是 ",
            );
    }
    if text.ends_with(" already exists; choose another folder or use --replace") {
        return text.replace(
            " already exists; choose another folder or use --replace",
            " 已存在；请选择其他文件夹或使用 --replace",
        );
    }
    if let Some(path) = text.strip_suffix(" (folder isn't there)") {
        return format!("{path}（文件夹不存在）");
    }
    if text.contains(": unknown (") {
        return text.replace(": unknown (", "：未知（");
    }
    if text.contains(" session entries, ") && text.contains(" saved filters, in ") {
        return text
            .replace(" session entries, ", " 个会话条目，")
            .replace(" saved filters, in ", " 个已保存筛选视图，位置：");
    }
    if text.contains(" bytes in ") && text.ends_with(" files") {
        return text
            .replace(" bytes in ", " 字节，")
            .replace(" files", " 个文件");
    }
    if text.ends_with(" sessions") {
        return text
            .strip_suffix(" sessions")
            .map_or_else(|| text.to_string(), |head| format!("{head} 个会话"));
    }
    if text.ends_with(" session") {
        return text
            .strip_suffix(" session")
            .map_or_else(|| text.to_string(), |head| format!("{head} 个会话"));
    }
    text.to_string()
}

pub fn human(text: &str) -> String {
    if locale() == Locale::English {
        return text.to_string();
    }
    exact(text)
        .map(String::from)
        .unwrap_or_else(|| dynamic(text))
}

fn help_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    if let Some(rest) = trimmed.strip_prefix("usage: ") {
        return format!("{indent}用法：{rest}");
    }
    let content = trimmed.trim_end();
    let suffix = &trimmed[content.len()..];
    if let Some(help) = content.strip_prefix("-h, --help") {
        return format!("{indent}-h, --help{}", help_line(help));
    }
    if let Some(help) = content.strip_prefix("-j, --json") {
        return format!("{indent}-j, --json{}", help_line(help));
    }
    let translated = match content {
        "positional arguments:" => "位置参数：",
        "options:" => "选项：",
        "show this help message and exit" => "显示帮助并退出",
        "Pin project folders as workspaces. The desktop app shows their bindings and" => {
            "固定项目文件夹作为工作区。桌面应用会显示绑定和"
        }
        "recent sessions, and starts a new session only in the agent and profile you" => {
            "最近会话；新会话只会在你明确选择的智能体和配置中启动。"
        }
        "choose. Pinning changes no binding or agent configuration." => {
            "固定操作不会改变绑定或智能体配置。"
        }
        "list pinned workspaces (the default)" => "列出固定的工作区（默认）",
        "pin a folder, or change its display name" => "固定文件夹或更改其显示名称",
        "unpin a folder without deleting it" => "取消固定文件夹，但不删除它",
        "list the workspaces as JSON" => "以 JSON 列出工作区",
        "Keep favorites, aliases, tags, archive state and saved session filters in" => {
            "在 Hatcheck 自己的 library.json 中保存收藏、别名、标签、归档状态和筛选视图。"
        }
        "Hatcheck's own library.json. No agent transcript or setting is changed, and" => {
            "不会修改任何智能体的会话记录或设置，"
        }
        "archiving never deletes a session." => "归档也绝不会删除会话。",
        "set                 change a session's alias, favorite, tags or archive" => {
            "set                 修改会话别名、收藏、标签或归档状态"
        }
        "state" => "状态",
        "clear               remove all Hatcheck metadata for a session" => {
            "clear               删除会话的全部 Hatcheck 元数据"
        }
        "save-filter         save a reusable desktop session filter" => {
            "save-filter         保存可复用的桌面会话筛选视图"
        }
        "remove-filter       remove a saved filter" => "remove-filter       删除已保存的筛选视图",
        "print the library or changed item as JSON" => "以 JSON 输出会话库或修改后的条目",
        "Export one session or a project/date batch as a self-contained Markdown, HTML" => {
            "将单个会话或按项目、日期筛选的会话导出为独立的 Markdown、HTML"
        }
        "or JSON document. Every document includes source and destination metadata and" => {
            "或 JSON 文档。每份文档都包含来源、目标元数据，"
        }
        "a warning that sessions may contain secrets or private source code." => {
            "以及会话可能包含密钥或私有源代码的警告。"
        }
        "one session ID or its start; leave out for a batch" => {
            "一个会话 ID 或其前缀；留空则批量导出"
        }
        "the profile or home to read SESSION from" => "读取会话的配置或主目录",
        "export only sessions started in this project folder" => "只导出在此项目文件夹中开始的会话",
        "markdown (the default), html or json" => "markdown（默认）、html 或 json",
        "a file for one session, or a directory for a batch or one" => {
            "单会话的输出文件，或批量及单会话的输出目录"
        }
        "session" => "会话",
        "replace files that already exist" => "替换已存在的文件",
        "export the handover brief instead of the whole conversation" => {
            "导出交接摘要，而不是完整对话"
        }
        "include Desktop, SDK and other non-CLI sessions in a batch" => {
            "批量导出时包含桌面、SDK 和其他非命令行会话"
        }
        "report what would be exported without writing files" => "报告将导出的内容，但不写入文件",
        "print the export report as JSON; --output is required" => {
            "以 JSON 输出导出报告；必须指定 --output"
        }
        "Show local plan-limit history, run-out and unused-quota forecasts, spending by" => {
            "显示本地套餐额度历史、提前用尽和未使用配额预测，以及按配置、"
        }
        "profile, project and model, and a weekly session summary. Reads only" => {
            "项目和模型划分的费用与每周会话摘要。只读取"
        }
        "Hatcheck's history and agents' session files; asks for no live usage." => {
            "Hatcheck 历史和智能体会话文件；不请求实时用量。"
        }
        "whole days of history to include, 1 to 30 (default 7)" => {
            "包含的历史天数，1 到 30（默认 7）"
        }
        "flag a limit resetting soon with at least this much left" => {
            "当额度即将重置且剩余至少达到此比例时提示"
        }
        "(default 50; 100 effectively disables it)" => "（默认 50；100 相当于禁用）",
        "print the report as JSON" => "以 JSON 输出报告",
        "--new PROFILE    start a fresh session in this explicitly chosen profile or" => {
            "--new PROFILE    在明确选择的配置或主目录中开始新会话，"
        }
        "home instead of resuming" => "而不是继续已有会话",
        "--cwd FOLDER     the project folder for --new (default: this folder)" => {
            "--cwd FOLDER     --new 使用的项目文件夹（默认：当前文件夹）"
        }
        _ => return line.to_string(),
    };
    format!("{indent}{translated}{suffix}")
}

fn help_with_locale(text: &str, language: Locale) -> String {
    if language == Locale::English {
        return text.to_string();
    }
    text.split_inclusive('\n').map(help_line).collect()
}

pub fn help(text: &str) -> String {
    help_with_locale(text, locale())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_keep_dynamic_values() {
        assert_eq!(
            dynamic("Pinned workspace \"API\" at ~/code/api."),
            "已固定工作区 \"API\"，位置：~/code/api."
        );
        assert_eq!(
            dynamic("Usage insights, last 7 days (local only)"),
            "本地用量分析：最近 7 天"
        );
        assert_eq!(
            dynamic("Backup saved in ~/backups/a."),
            "备份已保存到 ~/backups/a."
        );
        assert_eq!(
            dynamic("Project folder ~/x isn't there"),
            "项目文件夹 ~/x 不存在"
        );
    }

    #[test]
    fn help_changes_only_prose() {
        assert_eq!(help_line("options:"), "选项：");
        let help = help_with_locale("usage: hatcheck workspaces [-h]\n\noptions:\n  -h, --help  show this help message and exit\n", Locale::ZhCn);
        assert!(help.starts_with("用法：hatcheck workspaces"));
        assert!(help.contains("选项："));
        assert!(help.contains("显示帮助并退出"));
    }
}
