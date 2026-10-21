package com.codync.android.design

import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf

/** The language of this app's own interface. Chats and bots are not translated. */
enum class AppLanguage {
    System, Zh, En;

    fun usesChinese(systemIsChinese: Boolean): Boolean = when (this) {
        Zh -> true
        En -> false
        System -> systemIsChinese
    }

    companion object {
        fun fromStored(raw: String?): AppLanguage = when (raw) {
            "zh" -> Zh
            "en" -> En
            else -> System
        }

        fun stored(language: AppLanguage): String = when (language) {
            System -> "system"
            Zh -> "zh"
            En -> "en"
        }

        /** Language names stay in that language. "Follow system" is translated with the interface. */
        fun menuTitle(language: AppLanguage, preference: AppLanguage, systemIsChinese: Boolean, phrases: Map<String, String>): String =
            when (language) {
                System -> translate("Follow system", preference, systemIsChinese, phrases)
                Zh -> "中文"
                En -> "English"
            }
    }
}

fun translate(text: String, preference: AppLanguage, systemIsChinese: Boolean, phrases: Map<String, String>): String {
    if (!preference.usesChinese(systemIsChinese)) return text
    return phrases[text] ?: text
}

/** Defaults keep source strings, so a test that doesn't provide a language stays in English. */
val LocalAppLanguage = staticCompositionLocalOf { AppLanguage.System }
val LocalSystemChinese = staticCompositionLocalOf { false }
val LocalPhrases = staticCompositionLocalOf { emptyMap<String, String>() }
val LocalSetAppLanguage = staticCompositionLocalOf<(AppLanguage) -> Unit> { {} }

@Composable fun tr(text: String): String =
    translate(text, LocalAppLanguage.current, LocalSystemChinese.current, LocalPhrases.current)
