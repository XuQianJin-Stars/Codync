package com.codync.android

import android.content.Context
import com.codync.android.design.AppLanguage
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** This phone's interface language. The phrase table is the same file the iPhone app ships. */
internal object AppLanguageStore {
    private const val KEY = "appLanguage"
    private val phraseLock = Any()

    @Volatile private var cachedPhrases: Map<String, String>? = null

    private fun prefs(context: Context) = context.getSharedPreferences("codync-language", Context.MODE_PRIVATE)

    fun get(context: Context): AppLanguage = AppLanguage.fromStored(prefs(context).getString(KEY, null))

    fun set(context: Context, language: AppLanguage) {
        prefs(context).edit().putString(KEY, AppLanguage.stored(language)).apply()
    }

    /** A missing or malformed table is logged and left empty so the next call can try again. */
    fun phrases(context: Context): Map<String, String> {
        cachedPhrases?.let { return it }
        synchronized(phraseLock) {
            cachedPhrases?.let { return it }
            return try {
                val text = context.assets.open("zh-Hans.json").bufferedReader().use { it.readText() }
                val parsed = Json.parseToJsonElement(text).jsonObject.mapValues { (_, value) -> value.jsonPrimitive.content }
                cachedPhrases = parsed
                parsed
            } catch (error: Exception) {
                android.util.Log.e("Codync", "Chinese phrases failed to load", error)
                emptyMap()
            }
        }
    }
}
