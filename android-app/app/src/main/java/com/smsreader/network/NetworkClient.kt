package com.smsreader.network

import com.smsreader.model.SmsData
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.OutputStream
import java.net.Socket
import java.io.BufferedReader
import java.io.InputStreamReader
import org.json.JSONObject

class NetworkClient(private val host: String, private val port: Int) {
    private var socket: Socket? = null
    private var outputStream: OutputStream? = null
    private var reader: BufferedReader? = null
    private var commandListener: ((String) -> Unit)? = null
    private var onDisconnected: (() -> Unit)? = null
    @Volatile private var intentionallyDisconnecting = false
    @Volatile private var listenerThreadAlive = false

    fun setCommandListener(listener: (String) -> Unit) {
        commandListener = listener
    }
    
    fun setOnDisconnectedListener(listener: () -> Unit) {
        onDisconnected = listener
    }

    suspend fun connect(): Boolean {
        return withContext(Dispatchers.IO) {
            try {
                intentionallyDisconnecting = false
                socket = Socket(host, port)
                socket?.tcpNoDelay = true
                socket?.soTimeout = 30000
                outputStream = socket?.getOutputStream()
                reader = BufferedReader(InputStreamReader(socket?.getInputStream()))
                
                startCommandListener()
                true
            } catch (e: Exception) {
                e.printStackTrace()
                false
            }
        }
    }

    private fun startCommandListener() {
        listenerThreadAlive = true
        Thread {
            try {
                while (!intentionallyDisconnecting && socket?.isClosed == false) {
                    try {
                        val line = reader?.readLine()
                        if (line == null) {
                            break
                        }
                        commandListener?.invoke(line)
                    } catch (_: java.net.SocketTimeoutException) {
                        continue
                    }
                }
            } catch (e: Exception) {
                if (!intentionallyDisconnecting) {
                    e.printStackTrace()
                }
            } finally {
                listenerThreadAlive = false
                if (!intentionallyDisconnecting) {
                    onDisconnected?.invoke()
                }
            }
        }.start()
    }

    suspend fun send(smsData: SmsData): Boolean {
        return withContext(Dispatchers.IO) {
            try {
                val data = smsData.toJson() + "\n"
                outputStream?.write(data.toByteArray(Charsets.UTF_8))
                outputStream?.flush()
                true
            } catch (e: Exception) {
                e.printStackTrace()
                notifyDisconnected()
                false
            }
        }
    }

    suspend fun sendRaw(json: String): Boolean {
        return withContext(Dispatchers.IO) {
            try {
                val data = json + "\n"
                outputStream?.write(data.toByteArray(Charsets.UTF_8))
                outputStream?.flush()
                true
            } catch (e: Exception) {
                e.printStackTrace()
                notifyDisconnected()
                false
            }
        }
    }

    suspend fun sendPing(): Boolean {
        return withContext(Dispatchers.IO) {
            try {
                val json = JSONObject().apply { put("type", "ping") }
                outputStream?.write((json.toString() + "\n").toByteArray(Charsets.UTF_8))
                outputStream?.flush()
                true
            } catch (e: Exception) {
                e.printStackTrace()
                notifyDisconnected()
                false
            }
        }
    }

    fun disconnect() {
        intentionallyDisconnecting = true
        try {
            reader?.close()
            outputStream?.close()
            socket?.close()
        } catch (e: Exception) {
            e.printStackTrace()
        }
        reader = null
        outputStream = null
        socket = null
    }

    fun isConnected(): Boolean {
        return listenerThreadAlive && socket?.isConnected == true && socket?.isClosed == false
    }
    
    private fun notifyDisconnected() {
        if (!intentionallyDisconnecting) {
            onDisconnected?.invoke()
        }
    }
}