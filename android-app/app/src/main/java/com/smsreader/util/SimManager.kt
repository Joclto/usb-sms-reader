package com.smsreader.util

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.telephony.SubscriptionInfo
import android.telephony.SubscriptionManager
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
        
        try {
            val subscriptionInfoList: List<SubscriptionInfo>? = 
                subscriptionManager.activeSubscriptionInfoList
            
            subscriptionInfoList?.forEachIndexed { index, info ->
                val slotIndex = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    info.simSlotIndex
                } else {
                    index
                }
                
                val phoneNumber = try {
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        subscriptionManager.getPhoneNumber(info.subscriptionId)
                    } else {
                        info.number?.toString()
                    }
                } catch (e: Exception) {
                    null
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