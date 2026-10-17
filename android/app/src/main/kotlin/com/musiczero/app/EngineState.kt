package com.musiczero.app

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Polling state of the app talking to the engine socket. */
enum class Connection { Down, Starting, Up }

/**
 * Holds everything the screen renders and owns the polling loop. Control
 * commands fire and forget; the next poll reflects the effect exactly like
 * the CLI does.
 */
class EngineState(private val context: Context) {

    var status by mutableStateOf<Playing?>(null)
    var connection by mutableStateOf(Connection.Down)
    var modules by mutableStateOf<List<Manifest>>(emptyList())
    var playlists by mutableStateOf(Playlists("", emptyList()))
    var history by mutableStateOf<List<Track>>(emptyList())

    private var listedFor: Pair<String, String>? = null
    private var startedAt = 0L

    fun beginPolling(scope: CoroutineScope) {
        scope.launch(Dispatchers.IO) {
            Engine.prepare(context)
            modules = Engine.bundledModules(context)
            while (true) {
                tick()
                delay(1000)
            }
        }
    }

    private suspend fun tick() {
        val client = MzClient.default(context.cacheDir)
        val reply = client.call(request("status")) ?: run {
            if (connection == Connection.Up) {
                connection = Connection.Down
            }
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
            client.call(request("playlists"))?.let { listing ->
                playlists = MzClient.parsePlaylists(listing)
                listedFor = key
            }
        }
    }

    /** Spawns the engine when the socket is dead; at most once every 6 s. */
    private suspend fun maybeStartEngine() {
        if (modules.isEmpty()) {
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
            Engine.start(context, chosenModule())
        }
        if (!launched) {
            connection = Connection.Down
        }
    }

    fun chosenModule(): String {
        status?.module?.takeIf { it.isNotEmpty() }?.let { return it }
        val stored = context.getSharedPreferences("mz", Context.MODE_PRIVATE)
            .getString("module", null)
            ?.takeIf { value -> modules.any { it.id == value } }
        if (stored != null) {
            return stored
        }
        // local/icecast need ffmpeg data which the Android package lacks.
        modules.firstOrNull { it.id != "local" && it.id != "icecast" }?.let { return it.id }
        return modules.firstOrNull()?.id ?: "ymz"
    }

    fun toggle() = send("toggle")
    fun next() = send("next")
    fun previous() = send("previous")

    fun seekTo(milliseconds: Long) {
        if (milliseconds >= 0) {
            send("seek", value = milliseconds.toString(), key = "absolute")
        }
    }

    fun seekBy(seconds: Int) {
        // The wire format is microseconds; relative seeks signed via key.
        send("seek", value = (seconds * 1_000_000L).toString(), key = "relative")
    }

    fun switchTo(moduleId: String) {
        persistModule(moduleId)
        status = null
        listedFor = null
        send("switch", value = moduleId)
    }

    fun pickPlaylist(playlistId: String) {
        listedFor = null
        send("playlist", value = playlistId)
    }

    fun replay(track: Track) {
        // Shaped like the CLI: replay looks the track up in the active
        // module's history, matching how the tray replays entries.
        status?.module?.takeIf { it.isNotEmpty() }?.let { module ->
            send("replay", value = track.id, key = "id", module = module)
        }
    }

    private fun send(
        action: String,
        value: String = "",
        key: String = "",
        module: String = "",
    ) {
        CoroutineScope(Dispatchers.IO).launch {
            MzClient.default(context.cacheDir).call(request(action, value, key, module))
        }
    }

    private fun request(action: String, value: String = "", key: String = "", module: String = ""): JSONObject =
        JSONObject().apply {
            put("action", action)
            if (value.isNotEmpty()) put("value", value)
            if (key.isNotEmpty()) put("key", key)
            if (module.isNotEmpty()) put("module", module)
        }

    private fun persistModule(moduleId: String) {
        context.getSharedPreferences("mz", Context.MODE_PRIVATE)
            .edit()
            .putString("module", moduleId)
            .apply()
    }
}
