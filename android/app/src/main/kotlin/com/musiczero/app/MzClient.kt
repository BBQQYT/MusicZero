package com.musiczero.app

import android.net.LocalSocket
import android.net.LocalSocketAddress
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.DataInputStream
import java.io.File

/**
 * The one-line JSON control protocol shared by the mz CLI, the tray and this
 * app. The host listens on a unix socket inside our cache directory;
 * LocalSocket talks to it directly, so no subprocess is spawned per command.
 */
class MzClient(private val socketPath: File) {

    var lastError: String? = null
        private set

    /** Sends one request line and parses one reply line, or null on failure. */
    suspend fun call(request: JSONObject): JSONObject? = withContext(Dispatchers.IO) {
        try {
            LocalSocket().use { socket ->
                socket.connect(
                    LocalSocketAddress(socketPath.absolutePath, LocalSocketAddress.Namespace.FILESYSTEM),
                )
                socket.outputStream.apply {
                    write((request.toString() + "\n").toByteArray(Charsets.UTF_8))
                    flush()
                }
                val reader = DataInputStream(socket.inputStream)
                readLine(reader)?.let { JSONObject(it) }
            }
        } catch (failure: Exception) {
            lastError = failure.message ?: failure.toString()
            null
        }
    }

    private fun readLine(reader: DataInputStream): String? {
        val bytes = ArrayList<Byte>(1024)
        while (true) {
            val value = try {
                reader.read()
            } catch (_: Exception) {
                return null
            }
            when {
                value < 0 -> return if (bytes.isEmpty()) null else String(bytes.toByteArray(), Charsets.UTF_8)
                value == '\n'.code -> return String(bytes.toByteArray(), Charsets.UTF_8)
                value != '\r'.code -> bytes.add(value.toByte())
            }
        }
    }

    companion object {
        fun default(cacheDirectory: File): MzClient =
            MzClient(File(cacheDirectory, "musiczero.sock"))

        fun request(
            action: String,
            value: String = "",
            key: String = "",
            module: String = "",
        ): JSONObject = JSONObject().apply {
            put("action", action)
            if (value.isNotEmpty()) put("value", value)
            if (key.isNotEmpty()) put("key", key)
            if (module.isNotEmpty()) put("module", module)
        }

        /** Player snapshot; fields follow exactly the mz status JSON. */
        fun parseStatus(reply: JSONObject): Playing = Playing(
            module = reply.optString("module"),
            name = reply.optString("name"),
            playlist = reply.optString("playlist"),
            status = reply.optString("status"),
            trackId = reply.optString("track_id"),
            title = reply.optString("title"),
            artist = reply.optString("artist"),
            positionMs = reply.optLong("position_ms"),
            durationMs = reply.optLong("duration_ms"),
            canSeek = reply.optBoolean("can_seek"),
            canPrevious = reply.optBoolean("can_previous"),
            artUrl = reply.optString("art_url"),
        )

        /** The status reply embeds the recent-track list under "history". */
        fun parseHistory(reply: JSONObject): List<Track> =
            (reply.optJSONArray("history") ?: JSONArray()).toTracks()

        fun parsePlaylists(reply: JSONObject): Playlists = Playlists(
            playlist = reply.optString("playlist"),
            playlists = (reply.optJSONArray("playlists") ?: JSONArray()).toPlaylists(),
        )
    }
}

private fun JSONArray.toPlaylists(): List<Playlist> {
    val result = mutableListOf<Playlist>()
    for (index in 0 until length()) {
        optJSONObject(index)?.let { element ->
            result.add(Playlist(id = element.optString("id"), name = element.optString("name")))
        }
    }
    return result
}

private fun JSONArray.toTracks(): List<Track> {
    val result = mutableListOf<Track>()
    for (index in 0 until length()) {
        optJSONObject(index)?.let { element ->
            result.add(
                Track(
                    id = element.optString("id"),
                    title = element.optString("title"),
                    artist = element.optString("artist"),
                    artUrl = element.optString("art_url"),
                ),
            )
        }
    }
    return result
}

data class Playing(
    val module: String,
    val name: String,
    val playlist: String,
    val status: String,
    val trackId: String,
    val title: String,
    val artist: String,
    val positionMs: Long,
    val durationMs: Long,
    val canSeek: Boolean,
    val canPrevious: Boolean,
    val artUrl: String,
)

data class Playlist(val id: String, val name: String)

data class Playlists(val playlist: String, val playlists: List<Playlist>)

data class Track(val id: String, val title: String, val artist: String, val artUrl: String)

data class Manifest(
    val id: String,
    val name: String,
    val binary: String,
    val defaultPlaylist: String,
    val json: String = "",
)
