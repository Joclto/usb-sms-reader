package com.smsreader.util

import android.os.Handler
import android.os.Looper
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

object LogManager {
    private val logs = mutableListOf<String>()
    private const val MAX_LOGS = 200
    private var logListener: ((String) -> Unit)? = null
    private val dateFormat = SimpleDateFormat("HH:mm:ss.SSS", Locale.getDefault())
    
    fun log(message: String) {
        val timestamp = dateFormat.format(Date())
        val logMessage = "[$timestamp] $message"
        
        synchronized(logs) {
            logs.add(0, logMessage)
            if (logs.size > MAX_LOGS) {
                logs.removeAt(logs.size - 1)
            }
        }
        
        android.util.Log.d("SMSReader", message)
        
        Handler(Looper.getMainLooper()).post {
            logListener?.invoke(logMessage)
        }
    }
    
    fun logError(message: String, error: Throwable? = null) {
        val errorMsg = if (error != null) "$message: ${error.message}" else message
        log("❌ $errorMsg")
        error?.printStackTrace()
    }
    
    fun logSuccess(message: String) {
        log("✓ $message")
    }
    
    fun logInfo(message: String) {
        log("ℹ $message")
    }
    
    fun logWarning(message: String) {
        log("⚠ $message")
    }
    
    fun getLogs(): List<String> = synchronized(logs) { logs.toList() }
    
    fun clearLogs() {
        synchronized(logs) { logs.clear() }
        log("日志已清空")
    }
    
    fun setListener(listener: (String) -> Unit) {
        logListener = listener
    }
    
    fun removeListener() {
        logListener = null
    }
}