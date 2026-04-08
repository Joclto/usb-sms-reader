package com.smsreader.util

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.telephony.SubscriptionInfo
import android.telephony.SubscriptionManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat
import com.smsreader.model.SimCard

class SimManager(private val context: Context) {
    
    fun hasPhonePermissions(): Boolean {
        return ContextCompat.checkSelfPermission(context, Manifest.permission.READ_PHONE_STATE) == PackageManager.PERMISSION_GRANTED &&
               ContextCompat.checkSelfPermission(context, Manifest.permission.READ_PHONE_NUMBERS) == PackageManager.PERMISSION_GRANTED
    }
    
    @Suppress("DEPRECATION")
    fun getSimCards(): List<SimCard> {
        if (!hasPhonePermissions()) {
            return emptyList()
        }
        
        val simCards = mutableListOf<SimCard>()
        val subscriptionManager = context.getSystemService(Context.TELEPHONY_SUBSCRIPTION_SERVICE) as SubscriptionManager
        val telephonyManager = context.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager
        
        try {
            val subscriptionInfoList: List<SubscriptionInfo>? = 
                subscriptionManager.activeSubscriptionInfoList
            
            subscriptionInfoList?.forEachIndexed { index, info ->
                val slotIndex = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    info.simSlotIndex
                } else {
                    index
                }
                
                var phoneNumber: String? = null
                
                // Method 1: SubscriptionManager.getPhoneNumber (API 33+)
                if (phoneNumber.isNullOrEmpty() && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    try {
                        phoneNumber = subscriptionManager.getPhoneNumber(info.subscriptionId)
                    } catch (_: Exception) {}
                }
                
                // Method 2: SubscriptionInfo.getNumber (deprecated but works on some devices)
                if (phoneNumber.isNullOrEmpty()) {
                    try {
                        phoneNumber = info.number?.toString()
                    } catch (_: Exception) {}
                }
                
                // Method 3: TelephonyManager for slot 0
                if (phoneNumber.isNullOrEmpty() && slotIndex == 0) {
                    try {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                            val tmForSlot = telephonyManager.createForSubscriptionId(info.subscriptionId)
                            phoneNumber = tmForSlot.line1Number
                        } else {
                            phoneNumber = telephonyManager.line1Number
                        }
                    } catch (_: Exception) {}
                }
                
                // Method 4: TelephonyManager for slot 1 (dual SIM)
                if (phoneNumber.isNullOrEmpty() && slotIndex == 1 && Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    try {
                        val tmForSlot = telephonyManager.createForSubscriptionId(info.subscriptionId)
                        phoneNumber = tmForSlot.line1Number
                    } catch (_: Exception) {}
                }
                
                val carrierName = info.carrierName?.toString() ?: ""
                
                simCards.add(SimCard(
                    slotIndex = slotIndex,
                    phoneNumber = phoneNumber,
                    carrierName = carrierName,
                    isActive = true
                ))
            }
        } catch (e: Exception) {
            e.printStackTrace()
        }
        
        return simCards
    }
    
    fun getSimCardBySlot(slotIndex: Int): SimCard? {
        return getSimCards().find { it.slotIndex == slotIndex }
    }
}