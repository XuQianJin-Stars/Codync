package com.codync.android

import com.codync.android.design.AppLanguage
import com.codync.android.design.translate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppLanguageTest {
    private val phrases = mapOf("Pair a computer" to "配对一台电脑", "Follow system" to "跟随系统")

    @Test fun chineseFollowsTheChoiceNotTheOtherLanguage() {
        assertTrue(AppLanguage.Zh.usesChinese(systemIsChinese = false))
        assertFalse(AppLanguage.En.usesChinese(systemIsChinese = true))
        assertTrue(AppLanguage.System.usesChinese(systemIsChinese = true))
        assertFalse(AppLanguage.System.usesChinese(systemIsChinese = false))
    }

    @Test fun phrasesStayEnglishUntilChineseIsChosen() {
        assertEquals("配对一台电脑", translate("Pair a computer", AppLanguage.Zh, systemIsChinese = false, phrases))
        assertEquals("Pair a computer", translate("Pair a computer", AppLanguage.En, systemIsChinese = true, phrases))
        assertEquals("跟随系统", AppLanguage.menuTitle(AppLanguage.System, AppLanguage.Zh, systemIsChinese = false, phrases))
        assertEquals("中文", AppLanguage.menuTitle(AppLanguage.Zh, AppLanguage.En, systemIsChinese = true, phrases))
        assertEquals("Not a phrase", translate("Not a phrase", AppLanguage.Zh, systemIsChinese = false, phrases))
    }
}
