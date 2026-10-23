import Foundation
import Testing
@testable import CodyncKit

struct AppLanguageTests {
    @Test func chineseFollowsTheChoiceNotTheOtherLanguage() {
        #expect(AppLanguage.usesChinese(preference: .zh, systemIsChinese: false))
        #expect(!AppLanguage.usesChinese(preference: .en, systemIsChinese: true))
        #expect(AppLanguage.usesChinese(preference: .system, systemIsChinese: true))
        #expect(!AppLanguage.usesChinese(preference: .system, systemIsChinese: false))
    }

    @Test func systemChineseIsThePreferredLanguagePrefix() {
        #expect(AppLanguage.systemIsChinese(languages: ["zh-Hans-CN", "en"]))
        #expect(AppLanguage.systemIsChinese(languages: ["zh-Hant-TW"]))
        #expect(!AppLanguage.systemIsChinese(languages: ["en-US", "zh-Hans"]))
        #expect(!AppLanguage.systemIsChinese(languages: []))
    }

    @Test func phrasesUseSimplifiedChineseOnlyWhenThatLanguageIsActive() {
        #expect(L10n.tr("Pair a computer", preference: .zh, systemIsChinese: false) == "配对一台电脑")
        #expect(L10n.tr("Pair a computer", preference: .en, systemIsChinese: true) == "Pair a computer")
        #expect(L10n.tr("Pair a computer", preference: .system, systemIsChinese: true) == "配对一台电脑")
        #expect(L10n.tr("Not a phrase", preference: .zh, systemIsChinese: false) == "Not a phrase")
        #expect(AppLanguage.menuTitle(.zh, preference: .en, systemIsChinese: true) == "中文")
        #expect(AppLanguage.menuTitle(.system, preference: .zh, systemIsChinese: false) == "跟随系统")
    }

    @Test func usageCaptionAndScannerHintTranslateAsTheirOwnPhrases() {
        #expect(L10n.tr("Weekly", preference: .zh, systemIsChinese: false) == "每周")
        #expect(L10n.tr("highest usage", preference: .zh, systemIsChinese: false) == "用量最高")
        #expect(L10n.tr("Weekly · highest usage", preference: .zh, systemIsChinese: false) == "Weekly · highest usage")
        #expect(L10n.tr("Find nearby barcodes.", preference: .zh, systemIsChinese: false) == "寻找附近的条形码。")
        #expect(L10n.tr("Open Codync to connect a computer and check {provider} usage.", preference: .zh, systemIsChinese: false)
            == "打开 Codync，连接一台电脑后即可查看 {provider} 用量。")
    }

    @Test func settingsAndBotChromeTranslateAsTheirOwnPhrases() {
        #expect(L10n.tr("Personal", preference: .zh, systemIsChinese: false) == "个人")
        #expect(L10n.tr("This Mac", preference: .zh, systemIsChinese: false) == "这台 Mac")
        #expect(L10n.tr("Show in notch", preference: .zh, systemIsChinese: false) == "在屏幕顶部显示")
        #expect(L10n.tr("Privacy", preference: .zh, systemIsChinese: false) == "隐私")
        #expect(L10n.tr("Share usage analytics", preference: .zh, systemIsChinese: false) == "共享用量分析")
        #expect(L10n.tr("Personal workspace", preference: .zh, systemIsChinese: false) == "个人工作区")
        #expect(L10n.tr("Fast mode", preference: .zh, systemIsChinese: false) == "快速模式")
        #expect(L10n.tr("Use the computer", preference: .zh, systemIsChinese: false) == "使用电脑")
        #expect(L10n.tr("{size} pt (Default)", preference: .zh, systemIsChinese: false) == "{size} pt（默认）")
        #expect(L10n.tr("Cursor · Personal workspace", preference: .zh, systemIsChinese: false) == "Cursor · Personal workspace")
    }

    @Test func chatMenuAndMemoryTranslateAsTheirOwnPhrases() {
        #expect(L10n.tr("Full conversation", preference: .zh, systemIsChinese: false) == "完整对话")
        #expect(L10n.tr("Settings", preference: .zh, systemIsChinese: false) == "设置")
        #expect(L10n.tr("New session", preference: .zh, systemIsChinese: false) == "新会话")
        #expect(L10n.tr("Delete bot", preference: .zh, systemIsChinese: false) == "删除 Bot")
        #expect(L10n.tr("Connected", preference: .zh, systemIsChinese: false) == "已连接")
        #expect(L10n.tr("Ask {name}", preference: .zh, systemIsChinese: false) == "问 {name}")
        #expect(L10n.tr("Said", preference: .zh, systemIsChinese: false) == "说了")
        #expect(L10n.tr("Stopped.", preference: .zh, systemIsChinese: false) == "已停止。")
        #expect(L10n.tr("Today", preference: .zh, systemIsChinese: false) == "今天")
        #expect(L10n.tr("Yesterday", preference: .zh, systemIsChinese: false) == "昨天")
        #expect(L10n.tr("No dreams yet.", preference: .zh, systemIsChinese: false) == "还没有梦境。")
        #expect(L10n.tr("Standing instructions", preference: .zh, systemIsChinese: false) == "常驻指令")
        #expect(L10n.tr("Agent", preference: .zh, systemIsChinese: false) == "智能体")
        #expect(L10n.tr("Manage the team", preference: .zh, systemIsChinese: false) == "管理团队")
        #expect(L10n.tr("Skills", preference: .zh, systemIsChinese: false) == "技能")
        #expect(L10n.tr("Search agents, connectors and skills", preference: .zh, systemIsChinese: false) == "搜索智能体、连接器和技能")
        #expect(L10n.tr("Apps", preference: .zh, systemIsChinese: false) == "应用")
        #expect(L10n.tr("Agents", preference: .zh, systemIsChinese: false) == "智能体")
        #expect(L10n.tr("Installed", preference: .zh, systemIsChinese: false) == "已安装")
        #expect(L10n.tr("Not installed", preference: .zh, systemIsChinese: false) == "未安装")
        #expect(L10n.tr("Set up", preference: .zh, systemIsChinese: false) == "设置")
        #expect(L10n.tr("Writing a reply…", preference: .zh, systemIsChinese: false) == "正在写回复…")
        #expect(L10n.tr("Downloading Codebuddy Code…", preference: .zh, systemIsChinese: false) == "正在下载 Codebuddy Code…")
        #expect(L10n.tr("Needs approval: Bash", preference: .zh, systemIsChinese: false) == "需要批准：Bash")
        #expect(L10n.tr("Storage", preference: .zh, systemIsChinese: false) == "存储")
        #expect(L10n.tr("New skill", preference: .zh, systemIsChinese: false) == "新建技能")
        #expect(L10n.tr("Composio API key", preference: .zh, systemIsChinese: false) == "Composio API 密钥")
        #expect(L10n.tr("When to use it", preference: .zh, systemIsChinese: false) == "何时使用")
        #expect(L10n.tr("Add connector", preference: .zh, systemIsChinese: false) == "添加连接器")
        #expect(L10n.tr("Name your group", preference: .zh, systemIsChinese: false) == "给群聊起个名字")
        #expect(L10n.tr("Nothing installed", preference: .zh, systemIsChinese: false) == "还没有安装")
        #expect(L10n.tr("No setup needed.", preference: .zh, systemIsChinese: false) == "无需设置。")
        #expect(L10n.tr("FORWARDXU-MC3 · Connected", preference: .zh, systemIsChinese: false) == "FORWARDXU-MC3 · Connected")
    }
}
