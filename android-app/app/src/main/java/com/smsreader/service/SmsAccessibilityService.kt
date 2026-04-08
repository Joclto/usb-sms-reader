package com.smsreader.service

import android.accessibilityservice.AccessibilityService
import android.content.pm.PackageManager
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.provider.Telephony
import android.view.accessibility.AccessibilityEvent
import android.app.Notification
import android.Manifest
import com.smsreader.model.SmsData
import com.smsreader.network.NetworkClient
import com.smsreader.util.SimManager
import com.smsreader.util.LogManager
import com.smsreader.util.ConnectionState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.Job
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.util.concurrent.atomic.AtomicBoolean

class SmsAccessibilityService : AccessibilityService() {

    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var networkClient: NetworkClient? = null
    private val defaultHost = "127.0.0.1"
    private val defaultPort = 8080
    private var simManager: SimManager? = null
    private var connectionJob: Job? = null
    private var heartbeatJob: Job? = null
    private val heartbeatInterval = 10000L
    private var lastDisconnectTime = 0L
    private val handshakeConfirmed = AtomicBoolean(false)

    private val smsPackages = setOf(
        "com.android.mms",
        "com.android.mms.service",
        "com.google.android.apps.messaging",
        "com.samsung.android.messaging",
        "com.android.messaging",
        "com.android.providers.telephony"
    )

    override fun onServiceConnected() {
        super.onServiceConnected()
        LogManager.logSuccess("无障碍服务已连接")
        
        networkClient = NetworkClient(defaultHost, defaultPort)
        simManager = SimManager(applicationContext)
        
        networkClient?.setCommandListener { command ->
            handleCommand(command)
        }
        
        networkClient?.setOnDisconnectedListener {
            if (ConnectionState.isConnected) {
                LogManager.logWarning("连接已断开")
                ConnectionState.setConnected(false, "连接断开")
                heartbeatJob?.cancel()
            }
            lastDisconnectTime = System.currentTimeMillis()
        }
        
        startConnectionLoop()
    }

    private fun startConnectionLoop() {
        connectionJob?.cancel()
        connectionJob = serviceScope.launch {
            LogManager.logInfo("启动连接循环，目标: $defaultHost:$defaultPort")
            
            while (isActive) {
                try {
                    if (!ConnectionState.isConnected && !ConnectionState.isVerifying) {
                        val timeSinceDisconnect = System.currentTimeMillis() - lastDisconnectTime
                        if (lastDisconnectTime > 0 && timeSinceDisconnect < 5000) {
                            delay(5000 - timeSinceDisconnect)
                            continue
                        }
                        
                        val attempt = ConnectionState.connectionAttempts + 1
                        LogManager.log("尝试连接 #$attempt -> $defaultHost:$defaultPort")
                        
                        networkClient?.disconnect()
                        handshakeConfirmed.set(false)
                        val startTime = System.currentTimeMillis()
                        val success = networkClient?.connect() ?: false
                        val elapsed = System.currentTimeMillis() - startTime
                        
                        if (success) {
                            ConnectionState.setVerifying()
                            LogManager.log("TCP连接成功，验证PC程序...")
                            
                            networkClient?.sendRaw("""{"type":"handshake"}""")
                            
                            var verified = false
                            for (i in 1..15) {
                                delay(200)
                                if (handshakeConfirmed.get()) {
                                    verified = true
                                    break
                                }
                                if (!networkClient?.isConnected()!!) break
                            }
                            
                            if (verified) {
                                ConnectionState.setConnected(true)
                                LogManager.logSuccess("连接成功! 耗时 ${elapsed}ms")
                                sendSimCardsInfo()
                                startHeartbeat()
                            } else {
                                LogManager.logWarning("握手验证失败，PC程序未响应")
                                networkClient?.disconnect()
                                ConnectionState.setConnected(false, "PC程序未响应")
                                lastDisconnectTime = System.currentTimeMillis()
                            }
                        } else {
                            LogManager.logError("连接失败 (耗时 ${elapsed}ms)")
                            ConnectionState.setConnected(false, "连接被拒绝")
                        }
                    }
                } catch (e: Exception) {
                    LogManager.logError("连接异常", e)
                    ConnectionState.setConnected(false, e.message)
                    networkClient?.disconnect()
                    heartbeatJob?.cancel()
                }
                
                delay(3000)
            }
        }
    }
    
    private fun startHeartbeat() {
        heartbeatJob?.cancel()
        heartbeatJob = serviceScope.launch {
            delay(5000)
            while (isActive && ConnectionState.isConnected) {
                try {
                    val success = networkClient?.sendPing() ?: false
                    if (!success) {
                        LogManager.logWarning("心跳发送失败，标记断开")
                        ConnectionState.setConnected(false, "心跳失败")
                        networkClient?.disconnect()
                        break
                    }
                } catch (e: kotlinx.coroutines.CancellationException) {
                    throw e
                } catch (e: Exception) {
                    LogManager.logError("心跳异常", e)
                    ConnectionState.setConnected(false, e.message)
                    networkClient?.disconnect()
                    break
                }
                delay(heartbeatInterval)
            }
        }
    }

    private fun sendSimCardsInfo() {
        LogManager.logInfo("获取SIM卡信息...")
        val simCards = simManager?.getSimCards() ?: emptyList()
        LogManager.log("检测到 ${simCards.size} 个SIM卡")
        
        simCards.forEach { sim ->
            val carrier = sim.carrierName ?: "未知运营商"
            val number = sim.phoneNumber?.takeIf { it.isNotEmpty() } ?: "未知号码"
            LogManager.log("  SIM${sim.slotIndex + 1}: $carrier ($number)")
        }
        
        val messagesArray = org.json.JSONArray()
        simCards.forEach { sim ->
            messagesArray.put(sim.toJsonObject())
        }
        
        val json = JSONObject().apply {
            put("type", "sim_cards")
            put("cards", messagesArray)
        }
        
        serviceScope.launch {
            val sent = networkClient?.sendRaw(json.toString()) ?: false
            if (sent) {
                LogManager.logSuccess("SIM卡信息已发送")
            } else {
                LogManager.logError("SIM卡信息发送失败")
            }
        }
    }

    private fun handleCommand(command: String) {
        LogManager.logInfo("收到PC命令: $command")
        try {
            val json = JSONObject(command)
            val type = json.optString("type")
            
            when (type) {
                "handshake_ack" -> {
                    handshakeConfirmed.set(true)
                    LogManager.log("收到握手确认")
                }
                "fetch_all_sms" -> {
                    LogManager.log("执行: 获取所有短信")
                    serviceScope.launch(Dispatchers.IO) {
                        val limit = json.optInt("limit", 0)
                        val smsList = fetchAllSms(limit)
                        LogManager.log("找到 ${smsList.size} 条短信")
                        sendSmsListToPc(smsList)
                    }
                }
                "fetch_sms" -> {
                    val limit = json.optInt("limit", 100)
                    LogManager.log("执行: 获取最近 $limit 条短信")
                    serviceScope.launch(Dispatchers.IO) {
                        val smsList = fetchAllSms(limit)
                        LogManager.log("找到 ${smsList.size} 条短信")
                        sendSmsListToPc(smsList)
                    }
                }
                "get_sim_cards" -> {
                    LogManager.log("执行: 获取SIM卡信息")
                    sendSimCardsInfo()
                }
                "ping" -> {
                    LogManager.log("执行: 心跳响应")
                    sendPong()
                }
                else -> {
                    LogManager.logWarning("未知命令类型: $type")
                }
            }
        } catch (e: Exception) {
            LogManager.logError("命令解析失败", e)
        }
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent) {
        if (event.eventType == AccessibilityEvent.TYPE_NOTIFICATION_STATE_CHANGED) {
            val packageName = event.packageName?.toString() ?: return

            if (smsPackages.contains(packageName) || packageName.contains("sms", ignoreCase = true)) {
                val notification = event.parcelableData as? Notification
                val smsData = extractSmsFromNotification(notification, event, packageName)
                
                smsData?.let {
                    LogManager.log("收到新短信 [${it.sender}]: ${it.body.take(30)}...")
                    sendToPc(it)
                }
            }
        }
    }

    override fun onInterrupt() {
        LogManager.logWarning("无障碍服务被中断")
    }

    override fun onDestroy() {
        super.onDestroy()
        LogManager.logWarning("无障碍服务销毁，停止连接")
        connectionJob?.cancel()
        heartbeatJob?.cancel()
        ConnectionState.setConnected(false, "服务销毁")
        networkClient?.disconnect()
    }

    private fun extractSmsFromNotification(
        notification: Notification?,
        event: AccessibilityEvent,
        packageName: String
    ): SmsData? {
        try {
            val extras = notification?.extras
            val sender = extras?.getString(Notification.EXTRA_TITLE)
                ?: event.text.firstOrNull()?.toString()
                ?: "Unknown"
            
            val body = extras?.getCharSequence(Notification.EXTRA_TEXT)?.toString()
                ?: event.text.drop(1).joinToString(" ")
                ?: ""

            return SmsData(
                sender = sender,
                body = body,
                timestamp = System.currentTimeMillis(),
                packageName = packageName
            )
        } catch (e: Exception) {
            LogManager.logError("解析短信通知失败", e)
            return null
        }
    }

    private fun sendToPc(smsData: SmsData) {
        serviceScope.launch {
            if (!ConnectionState.isConnected) {
                LogManager.logWarning("未连接，无法发送短信到PC")
                return@launch
            }
            val success = networkClient?.send(smsData) ?: false
            if (success) {
                LogManager.logSuccess("短信已发送到PC [${smsData.sender}]")
            } else {
                LogManager.logError("短信发送失败")
            }
        }
    }

    fun fetchAllSms(limit: Int = 0): List<SmsData> {
        if (checkCallingOrSelfPermission(Manifest.permission.READ_SMS) != PackageManager.PERMISSION_GRANTED) {
            LogManager.logError("没有READ_SMS权限，无法读取短信")
            return emptyList()
        }

        LogManager.logInfo("开始读取短信数据库...")
        val smsList = mutableListOf<SmsData>()
        
        val subscriptionManager = getSystemService(TELEPHONY_SUBSCRIPTION_SERVICE) as? android.telephony.SubscriptionManager
        val subIdToSlot = mutableMapOf<Int, Int>()
        
        @Suppress("DEPRECATION")
        val subInfoList = subscriptionManager?.activeSubscriptionInfoList
        subInfoList?.forEachIndexed { index, info ->
            val slotIndex = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                info.simSlotIndex
            } else {
                index
            }
            @Suppress("DEPRECATION")
            val subId = info.subscriptionId
            subIdToSlot[subId] = slotIndex
        }
        
        val uri: Uri = Telephony.Sms.CONTENT_URI
        val projection = arrayOf(
            Telephony.Sms._ID,
            Telephony.Sms.ADDRESS,
            Telephony.Sms.BODY,
            Telephony.Sms.DATE,
            Telephony.Sms.READ,
            Telephony.Sms.SUBSCRIPTION_ID
        )

        val sortOrder = if (limit > 0) {
            "${Telephony.Sms.DATE} DESC LIMIT $limit"
        } else {
            "${Telephony.Sms.DATE} DESC"
        }

        val cursor: Cursor? = contentResolver.query(
            uri,
            projection,
            null,
            null,
            sortOrder
        )

        cursor?.use {
            val idIndex = it.getColumnIndex(Telephony.Sms._ID)
            val addressIndex = it.getColumnIndex(Telephony.Sms.ADDRESS)
            val bodyIndex = it.getColumnIndex(Telephony.Sms.BODY)
            val dateIndex = it.getColumnIndex(Telephony.Sms.DATE)
            val readIndex = it.getColumnIndex(Telephony.Sms.READ)
            val subIdIndex = it.getColumnIndex(Telephony.Sms.SUBSCRIPTION_ID)

            while (it.moveToNext()) {
                val id = it.getLong(idIndex)
                val address = it.getString(addressIndex)
                val body = it.getString(bodyIndex)
                val date = it.getLong(dateIndex)
                val read = it.getInt(readIndex) == 1
                val subId = if (subIdIndex >= 0) it.getInt(subIdIndex) else -1
                val simSlot = subIdToSlot[subId] ?: -1

                smsList.add(SmsData(
                    id = id,
                    sender = address,
                    body = body,
                    timestamp = date,
                    packageName = "content://sms",
                    read = read,
                    simSlot = simSlot
                ))
            }
        }
        
        LogManager.log("从数据库读取 ${smsList.size} 条短信 (SIM映射: ${subIdToSlot.size}个)")
        return smsList
    }

    fun sendSmsListToPc(smsList: List<SmsData>) {
        serviceScope.launch {
            if (!ConnectionState.isConnected) {
                LogManager.logWarning("未连接，无法发送短信列表到PC")
                return@launch
            }
            
            LogManager.logInfo("准备发送 ${smsList.size} 条短信到PC...")
            
            val messagesArray = org.json.JSONArray()
            smsList.forEach { sms ->
                messagesArray.put(sms.toJsonObject())
            }
            
            val json = JSONObject().apply {
                put("type", "sms_list")
                put("messages", messagesArray)
            }
            
            val sent = networkClient?.sendRaw(json.toString()) ?: false
            
            if (sent) {
                LogManager.logSuccess("已发送 ${smsList.size} 条短信到PC")
            } else {
                LogManager.logError("短信列表发送失败")
            }
        }
    }

    private fun sendPong() {
        serviceScope.launch {
            val json = JSONObject().apply {
                put("type", "pong")
            }
            val sent = networkClient?.sendRaw(json.toString()) ?: false
            if (sent) {
                LogManager.logSuccess("心跳响应已发送")
            } else {
                LogManager.logError("心跳响应发送失败")
            }
        }
    }
}