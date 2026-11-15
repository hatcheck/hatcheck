"""Opt-in Simplified Chinese CLI output, with stable English, raw and JSON interfaces."""
import json
import os
import unittest

from helpers import FakeHome, rec, write

A = "aaaaaaaa-0000-4000-8000-000000000001"
LANG = "zh-CN"


class ChineseCli(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.api = h.folder("code", "api")
        h.session(self.work, self.api, A, "Costly session", 5,
                  extra=[rec(type="cost-state", sessionId=A, totalCostUSD=1.5)])

    def test_workspace_messages_help_and_json(self):
        code, out = self.h.run("workspaces", "add", self.api, "--name", "API", HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        self.assertIn("已固定工作区", out)
        self.assertIn(self.h.shown(self.api), out)
        code, out = self.h.run("workspaces", "remove", self.api, HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        self.assertIn("已取消固定工作区", out)
        code, out = self.h.run("workspaces", HATCHECK_LANG=LANG)
        self.assertIn("没有固定的工作区", out)
        code, out = self.h.run("workspaces", "--json", HATCHECK_LANG=LANG)
        data = json.loads(out)
        self.assertEqual(data, {"version": 1, "workspaces": []})
        code, out = self.h.run("workspaces", "--help", HATCHECK_LANG=LANG)
        self.assertIn("用法：hatcheck workspaces", out)
        self.assertIn("位置参数：", out)
        self.assertIn("选项：", out)
        self.assertIn("显示帮助并退出", out)
        self.assertIn("固定项目文件夹作为工作区", out)

    def test_library_and_saved_filter_messages(self):
        code, out = self.h.run("library", "set", A[:8], "--from", "work", "--favorite", HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        self.assertIn("已更新", out)
        self.assertIn("Hatcheck 会话库条目", out)
        code, out = self.h.run("library", "--json", HATCHECK_LANG=LANG)
        entry = next(iter(json.loads(out)["sessions"].values()))
        self.assertTrue(entry["favorite"])
        code, out = self.h.run("library", "save-filter", "Favorites", "--favorites", HATCHECK_LANG=LANG)
        self.assertIn("已保存筛选视图", out)
        code, out = self.h.run("library", "remove-filter", "Favorites", HATCHECK_LANG=LANG)
        self.assertIn("已删除筛选视图", out)
        code, out = self.h.run("library", "clear", A[:8], "--from", "work", HATCHECK_LANG=LANG)
        self.assertIn("已清除该会话", out)

    def test_insights_and_storage_are_localized_but_json_is_stable(self):
        code, out = self.h.run("insights", HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        self.assertIn("本地用量分析：最近 7 天", out)
        self.assertIn("没有读数不等于用量为零", out)
        self.assertIn("会话：1", out)
        self.assertIn("配置：", out)
        code, out = self.h.run("insights", "--json", HATCHECK_LANG=LANG)
        report = json.loads(out)
        self.assertEqual(report["weekly"]["session_count"], 1)
        self.assertEqual(report["weekly"]["costed_session_count"], 1)
        code, out = self.h.run("doctor", "--deep", HATCHECK_LANG=LANG)
        self.assertIn("深度检查：", out)
        self.assertIn("按配置或主目录统计的存储：", out)
        self.assertIn("字节", out)
        self.assertIn("个文件", out)

    def test_export_backup_and_raw_commands(self):
        output = os.path.join(self.h.root, "export.md")
        code, out = self.h.run("export", A[:8], "--from", "work", "--output", output, HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        self.assertIn("已导出", out)
        self.assertIn("markdown", out)
        code, out = self.h.run("export", A[:8], "--from", "work", "--output", output, "--json", HATCHECK_LANG=LANG)
        self.assertEqual(code, 1, out)  # it exists, and JSON would still be valid on success
        code, out = self.h.run("export", A[:8], "--from", "work", "--output", output, "--replace", "--json", HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertEqual(report["exported"][0]["format"], "markdown")
        code, out = self.h.run("open", "--new", "work", "--cwd", self.api, "--print", HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        self.assertTrue(out.startswith("cd "))
        self.assertNotIn("开始新的", out)
        self.assertIn("claude", out)

    def test_backup_message_and_unknown_locale_fallback(self):
        self.h.profile("other")
        code, out = self.h.run("move", A[:8], "other", "--from", "work", "--backup", HATCHECK_LANG=LANG)
        self.assertEqual(code, 0, out)
        self.assertIn("备份已保存到", out)
        code, out = self.h.run("workspaces", "add", self.api, "--name", "API", HATCHECK_LANG="fr-FR")
        self.assertIn('Pinned workspace "API"', out)
        code, out = self.h.run("workspaces", "remove", self.api, HATCHECK_LANG="fr-FR")
        self.assertIn('Unpinned workspace "API"', out)


if __name__ == "__main__":
    unittest.main()
