package com.smsreader.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.View
import android.widget.Button
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.smsreader.R
import com.smsreader.util.ConnectionState
import com.smsreader.util.LogManager
import com.smsreader.util.SimManager

class MainActivity : AppCompatActivity() {

    private lateinit var statusText: TextView
    private lateinit var connectionStatus: TextView
    private lateinit var connectionDot: View
    private lateinit var enableButton: Button
    private lateinit var clearLogButton: Button
    private lateinit var logText: TextView
    private lateinit var logScrollView: ScrollView
    private lateinit var simManager: SimManager
    private val handler = Handler(Looper.getMainLooper())

    companion object {
        const val PERMISSION_REQUEST_CODE = 100
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        statusText = findViewById(R.id.statusText)
        connectionStatus = findViewById(R.id.connectionStatus)
        connectionDot = findViewById(R.id.connectionDot)
        enableButton = findViewById(R.id.enableButton)
        clearLogButton = findViewById(R.id.clearLogButton)
        logText = findViewById(R.id.logText)
        logScrollView = findViewById(R.id.logScrollView)
        simManager = SimManager(this)

        enableButton.setOnClickListener {
            openAccessibilitySettings()
        }
        
        clearLogButton.setOnClickListener {
            LogManager.clearLogs()
            updateLogDisplay()
        }

        requestPermissions()
        
        // 监听连接状态变化
        ConnectionState.setListener { _ ->
            runOnUiThread {
                updateConnectionStatus()
            }
        }
        
        LogManager.setListener { _ ->
            runOnUiThread {
                updateLogDisplay()
            }
        }
        
        updateLogDisplay()
        LogManager.logInfo("应用启动")
        LogManager.log("当前版本: 1.0")
        
        handler.postDelayed(object : Runnable {
            override fun run() {
                updateConnectionStatus()
                updateLogDisplay()
                handler.postDelayed(this, 1000)
            }
        }, 1000)
    }
    
    private fun updateConnectionStatus() {
        val isConnected = ConnectionState.isConnected
        
        // 更新状态文字
        if (isConnected) {
            connectionStatus.text = "PC连接: 已连接 ✓"
            connectionStatus.setTextColor(getColor(android.R.color.holo_green_light))
        } else {
            val attempts = ConnectionState.connectionAttempts
            if (attempts > 0) {
                connectionStatus.text = "PC连接: 重试中 #$attempts"
            } else {
                connectionStatus.text = "PC连接: 未连接 ✗"
            }
            connectionStatus.setTextColor(getColor(android.R.color.holo_red_light))
        }
        
        // 更新圆点颜色
        val dotDrawable = connectionDot.background as? GradientDrawable
        val dotColor = if (isConnected) {
            getColor(android.R.color.holo_green_light)
        } else {
            getColor(android.R.color.holo_red_light)
        }
        dotDrawable?.setColor(dotColor)
    }
    
    private fun updateLogDisplay() {
        val logs = LogManager.getLogs()
        logText.text = if (logs.isEmpty()) "等待日志..." else logs.joinToString("\n")
        logScrollView.post {
            logScrollView.fullScroll(ScrollView.FOCUS_UP)
        }
    }

    private fun requestPermissions() {
        val permissions = arrayOf(
            Manifest.permission.READ_PHONE_STATE,
            Manifest.permission.READ_PHONE_NUMBERS,
            Manifest.permission.READ_SMS
        )
        
        val neededPermissions = permissions.filter {
            ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED
        }
        
        if (neededPermissions.isNotEmpty()) {
            LogManager.log("请求权限: ${neededPermissions.map { it.substringAfterLast(".") }}")
            ActivityCompat.requestPermissions(this, neededPermissions.toTypedArray(), PERMISSION_REQUEST_CODE)
        } else {
            LogManager.logSuccess("所有权限已授予")
            checkSimCards()
        }
    }
    
    private fun checkSimCards() {
        LogManager.logInfo("检查SIM卡...")
        val simCards = simManager.getSimCards()
        if (simCards.isEmpty()) {
            LogManager.logWarning("未检测到SIM卡")
        } else {
            simCards.forEach { sim ->
                val carrier = sim.carrierName ?: "未知运营商"
                val number = sim.phoneNumber?.takeIf { it.isNotEmpty() } ?: "未知号码"
                LogManager.log("SIM${sim.slotIndex + 1}: $carrier ($number)")
            }
            LogManager.logSuccess("检测到 ${simCards.size} 个SIM卡")
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == PERMISSION_REQUEST_CODE) {
            val granted = grantResults.all { it == PackageManager.PERMISSION_GRANTED }
            if (granted) {
                LogManager.logSuccess("权限授予成功")
                checkSimCards()
            } else {
                LogManager.logError("部分权限被拒绝")
                val denied = permissions.filterIndexed { idx, _ -> grantResults[idx] != PackageManager.PERMISSION_GRANTED }
                LogManager.log("被拒绝: ${denied.map { it.substringAfterLast(".") }}")
            }
        }
    }

    override fun onResume() {
        super.onResume()
        checkAccessibilityPermission()
    }
    
    override fun onDestroy() {
        super.onDestroy()
        ConnectionState.setListener {}
        handler.removeCallbacksAndMessages(null)
    }

    private fun checkAccessibilityPermission() {
        val enabled = isAccessibilityServiceEnabled()
        if (enabled) {
            statusText.text = "✓ 无障碍服务已启用"
            enableButton.isEnabled = false
            LogManager.logSuccess("无障碍服务已启用")
        } else {
            statusText.text = "✗ 请启用无障碍服务"
            enableButton.isEnabled = true
            LogManager.logWarning("无障碍服务未启用，请点击上方按钮开启")
        }
    }

    private fun isAccessibilityServiceEnabled(): Boolean {
        var accessibilityEnabled = 0
        try {
            accessibilityEnabled = Settings.Secure.getInt(
                contentResolver,
                Settings.Secure.ACCESSIBILITY_ENABLED
            )
        } catch (e: Settings.SettingNotFoundException) {
            e.printStackTrace()
        }

        if (accessibilityEnabled == 1) {
            val service = packageName + "/" + "com.smsreader.service.SmsAccessibilityService"
            val enabledServices = Settings.Secure.getString(
                contentResolver,
                Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES
            ) ?: return false

            return enabledServices.contains(service)
        }
        return false
    }

    private fun openAccessibilitySettings() {
        LogManager.log("打开无障碍设置...")
        val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
        startActivity(intent)
    }
}