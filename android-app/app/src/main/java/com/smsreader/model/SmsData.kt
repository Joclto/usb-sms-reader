package com.smsreader.model

import org.json.JSONObject

data class SmsData(
    val id: Long = 0,
    val sender: String,
    val body: String,
    val timestamp: Long,
    val packageName: String,
    val read: Boolean = false,
    val simSlot: Int = -1
) {
    fun toJson(): String {
        return toJsonObject().toString()
    }
    
    fun toJsonObject(): JSONObject {
        val json = JSONObject()
        json.put("id", id)
        json.put("sender", sender)
        json.put("body", body)
        json.put("timestamp", timestamp)
        json.put("packageName", packageName)
        json.put("read", read)
        json.put("simSlot", simSlot)
        return json
    }
}