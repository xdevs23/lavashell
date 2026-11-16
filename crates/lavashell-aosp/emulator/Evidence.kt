// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.hardware.HardwareBuffer
import org.json.JSONObject

internal fun Int.toHexColor() = "#%08x".format(this)

internal fun Long.toHexFlags() = "%x".format(this)

internal data class BufferDescription(val format: Int, val usage: Long) {
    fun toJson() = JSONObject(mapOf("format" to format, "usage" to usage.toHexFlags()))
}

internal fun HardwareBuffer.describe() = BufferDescription(format, usage)

internal data class StreamState(val drawnSize: PixelSize?, val wakes: Long) {
    fun toFields() = mapOf("drawnSize" to drawnSize?.let { listOf(it.width, it.height) }, "wakes" to wakes)
}

internal data class RunFailure(val failure: Throwable) : Evidence {
    override fun toJson() = JSONObject(mapOf("failure" to failure.toString()))
}

internal sealed class AcquiredImage {
    object NotDelivered : AcquiredImage()

    data class Delivered(val imageFormat: Int, val buffer: BufferDescription?, val middle: Int?) : AcquiredImage()

    data class Failed(val error: String) : AcquiredImage()
}

internal data class ReaderFormatEvidence(val options: ImageReaderOptions, val image: AcquiredImage) : Evidence {
    override fun toJson() = JSONObject(
        mapOf("readerFormat" to options.format, "readerUsage" to options.usage.toHexFlags()) + when (image) {
            AcquiredImage.NotDelivered -> mapOf("delivered" to false)
            is AcquiredImage.Delivered -> mapOf(
                "delivered" to true,
                "imageFormat" to image.imageFormat,
                "buffer" to (image.buffer?.toJson() ?: JSONObject.NULL),
            ) + (image.middle?.let { mapOf("middle" to it.toHexColor()) } ?: emptyMap())
            is AcquiredImage.Failed -> mapOf("acquireError" to image.error)
        },
    )
}

internal sealed class ColorExtra {
    abstract fun toFields(): Map<String, Any?>

    object None : ColorExtra() {
        override fun toFields() = emptyMap<String, Any?>()
    }

    data class Reached(val reached: Boolean) : ColorExtra() {
        override fun toFields() = mapOf("reached" to reached)
    }

    data class ReceivedMotions(val received: List<Boolean>) : ColorExtra() {
        override fun toFields() = mapOf("receivedByHost" to received)
    }

    data class Grouped(val outside: Int, val inside: Int) : ColorExtra() {
        override fun toFields() = mapOf("outside" to outside.toHexColor(), "inside" to inside.toHexColor())
    }

    data class Yuv(val buffer: BufferDescription, val conversion: YcbcrConversion) : ColorExtra() {
        override fun toFields() = mapOf("buffer" to buffer.toJson(), "conversion" to conversion.toJson())
    }
}

internal data class ColorEvidence(
    val state: StreamState,
    val expected: Int,
    val observed: Int,
    val extra: ColorExtra = ColorExtra.None,
) : Evidence {
    override fun toJson() = JSONObject(
        state.toFields() + mapOf("expected" to expected.toHexColor(), "observed" to observed.toHexColor()) +
            extra.toFields(),
    )
}

internal data class UnlatchedFramesEvidence(val state: StreamState, val drawsWhileUnlatched: Int) : Evidence {
    override fun toJson() = JSONObject(state.toFields() + mapOf("drawsWhileUnlatched" to drawsWhileUnlatched))
}

internal data class UnlatchedGpuFramesEvidence(
    val state: StreamState,
    val posted: Int,
    val single: Int,
    val batch: Int,
    val singleWakes: Long,
    val batchWakes: Long,
) : Evidence {
    override fun toJson() = JSONObject(
        state.toFields() + mapOf(
            "posted" to posted,
            "single" to single.toHexColor(),
            "batch" to batch.toHexColor(),
            "singleWakes" to singleWakes,
            "batchWakes" to batchWakes,
        ),
    )
}
