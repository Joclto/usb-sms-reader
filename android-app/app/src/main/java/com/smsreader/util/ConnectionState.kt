package com.smsreader.util

object ConnectionState {
    var isConnected: Boolean = false
        private set
    var lastError: String? = null
        private set
    var connectionAttempts: Int = 0
        private set
    
    private var listener: ((Boolean) -> Unit)? = null
    
    fun setConnected(connected: Boolean, error: String? = null) {
        val wasConnected = isConnected
        isConnected = connected
        lastError = if (!connected) error else null
        if (!connected) connectionAttempts++ else connectionAttempts = 0
        
        if (wasConnected != connected) {
            listener?.invoke(connected)
        }
    }
    
    fun setListener(l: (Boolean) -> Unit) {
        listener = l
    }
    
    fun getStatusText(): String {
        return if (isConnected) {
            "✓ 已连接"
        } else {
            "✗ 未连接 ${if (connectionAttempts > 0) "(重试 #$connectionAttempts)" else ""}"
        }
    }
}