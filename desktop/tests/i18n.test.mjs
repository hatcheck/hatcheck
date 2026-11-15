import test from "node:test";
import assert from "node:assert/strict";
import { translateText } from "../src/i18n.ts";

test("exact UI messages use the Chinese glossary", () => {
  assert.equal(translateText("Sessions", "zh-CN"), "会话");
  assert.equal(translateText("Every profile", "zh-CN"), "全部配置");
  assert.equal(translateText("No plan readings in this period. Hatcheck records them as it reads plans; no reading isn't zero usage.", "zh-CN"),
    "此时间段没有套餐读数。Hatcheck 会在读取套餐时记录；没有读数不等于用量为零。");
});

test("dynamic messages keep their values", () => {
  assert.equal(translateText("Export 3 shown…", "zh-CN"), "导出 3 个筛选结果…");
  assert.equal(translateText("Local insights · 7 days", "zh-CN"), "本地用量分析 · 最近 7 天");
  assert.equal(translateText("Move “Fix login” to another profile", "zh-CN"), "将“Fix login”移动到其他配置");
  assert.equal(translateText("60% left · resets tomorrow", "zh-CN"), "剩余 60% · 重置时间：tomorrow");
});

test("English and unknown text are unchanged", () => {
  assert.equal(translateText("Sessions", "en"), "Sessions");
  assert.equal(translateText("A provider-specific error", "zh-CN"), "A provider-specific error");
});
