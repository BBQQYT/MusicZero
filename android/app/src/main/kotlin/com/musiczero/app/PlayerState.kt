package com.musiczero.app

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Where the engine process currently is. */
enum class Connection { Down, Starting, Up }

/**
 * Everything the screens render plus the polling loop that keeps it fresh.
 * Control commands are fire-and-forget; the next poll reflects their effect,
 * exactly like the CLI behaves.
 */
class PlayerState(private val context: Context) {

    var status by mutableStateOf<Playing?>(null)
    var connection by mutableStateOf(Connection.Down)
    var modules by mutableStateOf<List<Manifest>>(emptyList())
    var playlists by mutableStateOf(Playlists("", emptyList()))
    var history by mutableStateOf<List<Track>>(emptyList())
    var settings by mutableStateOf<JSONObject?>(null)
    var settingsError by mutableStateOf<String?>(null)
    var suspended by mutableStateOf(Engine.isSuspended(context))

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var listedFor: Pair<String, String>? = null
    private var startedAt = 0L

    fun start() {
        scope.launch {
            Engine.prepare(context)
            modules = Engine.bundledModules(context)
            while (true) {
                tick()
                delay(1000)
            }
        }
    }

    private suspend fun client(): MzClient = MzClient.default(context.cacheDir)

    private suspend fun tick() {
        val socket = client()
        val reply = socket.call(MzClient.request("status")) ?: run {
            if (connection != Connection.Down) {
                status = null
            }
            connection = Connection.Down
            maybeStartEngine()
            return
        }
        val playing = MzClient.parseStatus(reply)
        status = playing
        history = MzClient.parseHistory(reply)
        connection = Connection.Up
        startedAt = 0
        val key = playing.module to playing.playlist
        if (listedFor != key) {
            socket.call(MzClient.request("playlists"))?.let { listing ->
                playlists = MzClient.parsePlaylists(listing)
                listedFor = key
            }
        }
    }

    /** Spawns the engine when the socket is dead; at most once every 6 s. */
    private suspend fun maybeStartEngine() {
        if (modules.isEmpty() || suspended) {
            return
        }
        val now = System.currentTimeMillis()
        if (startedAt != 0L && now - startedAt < 6_000) {
            connection = Connection.Starting
            return
        }
        startedAt = now
        connection = Connection.Starting
        val launched = withContext(Dispatchers.IO) {
            Engine.start(context, Engine.chosenModule(context, modules, status?.module))
        }
        if (!launched) {
            connection = Connection.Down
        }
    }

    fun moduleName(moduleId: String?): String? =
        modules.firstOrNull { it.id == moduleId }?.name

    fun toggle() = send("toggle")
    fun next() = send("next")
    fun previous() = send("previous")

    fun seekTo(milliseconds: Long) {
        if (milliseconds >= 0) {
            send("seek", value = milliseconds.toString(), key = "absolute")
        }
    }

    fun seekBy(seconds: Int) {
        // The wire format is microseconds, signed via the relative key.
        send("seek", value = (seconds * 1_000_000L).toString(), key = "relative")
    }

    fun switchTo(moduleId: String) {
        Engine.storeModule(context, moduleId)
        status = null
        listedFor = null
        settings = null
        settingsError = null
        if (connection == Connection.Up) {
            send("switch", value = moduleId)
        } else {
            startEngineNow(moduleId)
        }
    }

    fun pickPlaylist(playlistId: String) {
        listedFor = null
        send("playlist", value = playlistId, module = status?.module.orEmpty())
    }

    fun replay(track: Track) {
        status?.module?.takeIf { it.isNotEmpty() }?.let { module ->
            send("replay", value = track.id, key = "id", module = module)
        }
    }

    fun loadSettings() {
        scope.launch {
            if (connection != Connection.Up) {
                settings = null
                return@launch
            }
            val reply = client().call(MzClient.request("settings"))
            when {
                reply == null -> settingsError = "socket"
                reply.has("error") -> settingsError = reply.optString("error")
                else -> {
                    settingsError = null
                    settings = reply.optJSONObject("settings") ?: JSONObject()
                }
            }
        }
    }

    fun setSetting(key: String, value: String) {
        scope.launch {
            val reply = client().call(
                MzClient.request("set-setting", value = value, key = key, module = status?.module.orEmpty()),
            )
            when {
                reply == null -> settingsError = "socket"
                reply.has("error") -> settingsError = reply.optString("error")
                else -> {
                    settingsError = null
                    reply.optJSONObject("settings")?.let { settings = it }
                }
            }
        }
    }

    /** User pressed Stop: terminate gracefully and keep pollers from respawning. */
    fun stopEngine() {
        suspended = true
        Engine.setSuspended(context, true)
        scope.launch { client().call(MzClient.request("quit")) }
        Engine.stop()
        status = null
        connection = Connection.Down
    }

    fun startEngine() {
        startEngineNow(null)
    }

    private fun startEngineNow(moduleId: String?) {
        suspended = false
        Engine.setSuspended(context, false)
        connection = Connection.Starting
        startedAt = System.currentTimeMillis()
        scope.launch {
            Engine.start(context, moduleId ?: Engine.chosenModule(context, modules, status?.module))
        }
    }

    private fun send(
        action: String,
        value: String = "",
        key: String = "",
        module: String = "",
    ) {
        scope.launch {
            client().call(MzClient.request(action, value, key, module))
        }
    }
}
