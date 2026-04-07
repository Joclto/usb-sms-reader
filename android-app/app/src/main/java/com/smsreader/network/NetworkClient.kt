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

    fun setCommandListener(listener: (String) -> Unit) {
        commandListener = listener
    }

    suspend fun connect(): Boolean {
        return withContext(Dispatchers.IO) {
            try {
                socket = Socket(host, port)
                outputStream = socket?.getOutputStream()
                reader = BufferedReader(InputStreamReader(socket?.getInputStream()))
                
                // Start listening for commands
                startCommandListener()
                true
            } catch (e: Exception) {
                e.printStackTrace()
                false
            }
        }
    }

    private fun startCommandListener() {
        Thread {
            try {
                while (socket?.isConnected == true) {
                    val line = reader?.readLine() ?: break
                    commandListener?.invoke(line)
                }
            } catch (e: Exception) {
                e.printStackTrace()
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
                false
            }
        }
    }

    suspend fun readLine(): String? {
        return withContext(Dispatchers.IO) {
            try {
                reader?.readLine()
            } catch (e: Exception) {
                e.printStackTrace()
                null
            }
        }
    }

    fun disconnect() {
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
        return socket?.isConnected == true && socket?.isClosed == false
    }
}