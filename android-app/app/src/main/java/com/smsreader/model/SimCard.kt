package com.smsreader.model

import org.json.JSONObject

data class SimCard(
    val slotIndex: Int,
    val phoneNumber: String?,
    val carrierName: String?,
    val isActive: Boolean
) {
    fun toJson(): String {
        val json = JSONObject()
        json.put("slotIndex", slotIndex)
        json.put("phoneNumber", phoneNumber ?: "")
        json.put("carrierName", carrierName ?: "")
        json.put("isActive", isActive)
        return json.toString()
    }
    
    fun toJsonObject(): JSONObject {
        val json = JSONObject()
        json.put("slotIndex", slotIndex)
        json.put("phoneNumber", phoneNumber ?: "")
        json.put("carrierName", carrierName ?: "")
        json.put("isActive", isActive)
        return json
    }
    
    fun getDisplayName(): String {
        val number = phoneNumber?.takeIf { it.isNotEmpty() } ?: "SIM${slotIndex + 1}"
        val carrier = carrierName?.takeIf { it.isNotEmpty() } ?: ""
        return if (carrier.isNotEmpty() && number != "SIM${slotIndex + 1}") {
            "$number ($carrier)"
        } else if (carrier.isNotEmpty()) {
            "SIM${slotIndex + 1} ($carrier)"
        } else {
            number
        }
    }
}