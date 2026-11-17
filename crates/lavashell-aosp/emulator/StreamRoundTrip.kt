// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.content.Context
import android.hardware.HardwareBuffer
import android.view.MotionEvent
import android.view.Surface

internal class StreamRoundTrip(context: Context, sizes: BufferSizes, format: StreamFormat) : AutoCloseable {
    private val handle = create(
        context,
        sizes.default.width, sizes.default.height, sizes.max.width, sizes.max.height, format.id,
    )
    private var unreceivedWakesAtClose: Long? = null

    fun surface(): Surface = createSurface(handle)

    fun latch(): Unit = latch(handle)

    fun awaitWake(): Unit = awaitWake(handle)

    fun render(): Int = render(handle)

    fun renderGrouped(opacity: Float): IntArray = renderGrouped(handle, opacity)

    fun resize(sizes: BufferSizes): Unit = resize(
        handle, sizes.default.width, sizes.default.height, sizes.max.width, sizes.max.height,
    )

    fun encodeYcbcr(buffer: HardwareBuffer, color: Int): YcbcrConversion = encodeYcbcr(handle, buffer, color)

    fun reportMotion(event: MotionEvent): Boolean = reportMotion(handle, event)

    fun state() = StreamState(
        drawnSize = readDrawnSize(handle)?.let { (width, height) -> PixelSize(width.toInt(), height.toInt()) },
        wakes = countWakes(handle),
    )

    fun unreceivedWakes() = checkNotNull(unreceivedWakesAtClose) { "the round trip is still open" }

    override fun close() {
        if (unreceivedWakesAtClose == null) {
            unreceivedWakesAtClose = destroy(handle)
        }
    }

    private external fun create(
        context: Context,
        width: Int,
        height: Int,
        maxWidth: Int,
        maxHeight: Int,
        format: Int,
    ): Long

    private external fun createSurface(handle: Long): Surface

    private external fun latch(handle: Long)

    private external fun awaitWake(handle: Long)

    private external fun render(handle: Long): Int

    private external fun renderGrouped(handle: Long, opacity: Float): IntArray

    private external fun readDrawnSize(handle: Long): LongArray?

    private external fun countWakes(handle: Long): Long

    private external fun resize(handle: Long, width: Int, height: Int, maxWidth: Int, maxHeight: Int)

    private external fun destroy(handle: Long): Long

    private external fun encodeYcbcr(handle: Long, buffer: HardwareBuffer, color: Int): YcbcrConversion

    private external fun reportMotion(handle: Long, event: MotionEvent): Boolean

    private companion object {
        init {
            System.loadLibrary("surface_stream_round_trip")
        }
    }
}
