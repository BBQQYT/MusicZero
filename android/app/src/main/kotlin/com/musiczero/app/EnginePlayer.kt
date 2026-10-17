package com.musiczero.app

import android.net.Uri
import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import org.json.JSONObject

/**
 * Bridges the mz engine into a media3 Player so Android Auto, the lock screen
 * and the media session see one consistent playback picture. The engine does
 * the actual audio; this player only mirrors state and forwards commands to
 * the transport supplied by the service.
 */
class EnginePlayer(
    private val transport: (request: JSONObject) -> Unit,
) : SimpleBasePlayer(Looper.getMainLooper()) {

    private val uid = Any()
    private var item = MediaItem.EMPTY
    private var playing = false
    private var ready = false
    private var positionMs = 0L
    private var durationMs = 0L
    private var canSeek = false
    private var canPrevious = false
    private val done: ListenableFuture<Any?> = Futures.immediateFuture(null)

    /** Applies one engine status snapshot; must run on the main thread. */
    fun update(status: Playing?) {
        if (status == null) {
            ready = false
            invalidateState()
            return
        }
        ready = true
        playing = status.status == "Playing"
        positionMs = status.positionMs
        durationMs = status.durationMs
        canSeek = status.canSeek
        canPrevious = status.canPrevious
        item = MediaItem.Builder()
            .setMediaId(status.trackId.ifEmpty { "current" })
            .setMediaMetadata(
                MediaMetadata.Builder()
                    .setTitle(status.title.ifEmpty { status.name })
                    .setArtist(status.artist)
                    .setArtworkUri(if (status.artUrl.isEmpty()) null else Uri.parse(status.artUrl))
                    .build(),
            )
            .build()
        invalidateState()
    }

    override fun getState(): State {
        val data = SimpleBasePlayer.MediaItemData.Builder(uid)
            .setMediaItem(item)
            .setDurationUs(if (durationMs > 0) durationMs * 1000 else androidx.media3.common.C.TIME_UNSET)
            .setIsSeekable(canSeek)
            .setDefaultPositionUs(0)
            .build()
        return State.Builder()
            .setAvailableCommands(available())
            .setPlaybackState(if (ready) Player.STATE_READY else Player.STATE_IDLE)
            .setPlayWhenReady(playing, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            .setPlaylist(listOf(data))
            .setCurrentMediaItemIndex(0)
            .setContentPositionMs(positionMs.coerceAtLeast(0))
            .setSeekBackIncrementMs(15_000)
            .setSeekForwardIncrementMs(15_000)
            .build()
    }

    private fun available(): Player.Commands = Player.Commands.Builder()
        .addAll(
            Player.COMMAND_PLAY_PAUSE,
            Player.COMMAND_PREPARE,
            Player.COMMAND_STOP,
            Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
            Player.COMMAND_SEEK_BACK,
            Player.COMMAND_SEEK_FORWARD,
            Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
        )
        .addIf(Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM, canPrevious)
        .build()

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        playing = playWhenReady
        // The engine has dedicated play and pause actions, so no toggle
        // guesswork is needed.
        dispatch(if (playWhenReady) "play" else "pause")
        invalidateState()
        return done
    }

    override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekFlags: Int): ListenableFuture<*> {
        this.positionMs = positionMs
        // Wire format: microseconds with an absolute key, cf. mz seek.rs.
        transport(
            JSONObject().apply {
                put("action", "seek")
                put("value", positionMs.coerceAtLeast(0) * 1000)
                put("key", "absolute")
            },
        )
        invalidateState()
        return done
    }

    override fun handleStop(): ListenableFuture<*> {
        playing = false
        dispatch("stop")
        invalidateState()
        return done
    }

    override fun handleSetMediaItems(
        mediaItems: List<MediaItem>,
        startIndex: Int,
        startPositionMs: Long,
    ): ListenableFuture<*> {
        // Auto and controller surfaces enqueue from our browse tree; the
        // engine owns the real queue, so adopt the item metadata only. Real
        // playback actions arrive through the library callback below.
        mediaItems.firstOrNull()?.let { first ->
            item = first
            playing = true
            if (startPositionMs >= 0) {
                positionMs = startPositionMs
            }
            dispatch("play")
        }
        invalidateState()
        return done
    }

    private fun dispatch(action: String, value: String = "", key: String = "") {
        val request = JSONObject().apply {
            put("action", action)
            if (value.isNotEmpty()) put("value", value)
            if (key.isNotEmpty()) put("key", key)
        }
        transport(request)
    }
}
