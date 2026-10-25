// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.app.Activity
import android.app.Presentation
import android.content.res.Resources
import android.graphics.Color
import android.graphics.ColorSpace
import android.graphics.ImageFormat
import android.graphics.PixelFormat
import android.hardware.HardwareBuffer
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.Image
import android.media.ImageReader
import android.media.ImageWriter
import android.os.Handler
import android.os.Looper
import android.util.DisplayMetrics
import android.view.Surface
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.FutureTask
import kotlin.math.abs
import kotlin.math.max
import org.json.JSONObject

/**
 * Checks that colors survive a round trip through SurfaceStream. A frame is one image that a producer queues into the
 * Surface of a stream. Each frame here is filled with one known color. The native library surface_stream_round_trip
 * latches the newest frame of the stream, draws it into a pane with the Lavashell renderer and reads the pixels of the
 * pane back. A round trip passes when the color read back is the color that was sent. The frames come from three
 * producers: a VirtualDisplay that shows a Presentation, [SolidColorRenderer] and an ImageWriter. Each step adds one
 * line to the results: a JSON object with the fields step, matches and evidence.
 */
class StreamRoundTripActivity : Activity() {
    private val main = Handler(Looper.getMainLooper())
    private val results = StringBuilder()
    private val metrics: DisplayMetrics by lazy { resources.displayMetrics }
    private val displayWidth get() = metrics.widthPixels.toUInt()
    private val displayHeight get() = metrics.heightPixels.toUInt()
    private val longestSide get() = max(metrics.widthPixels, metrics.heightPixels).toUInt()

    /** Runs every step on the calling thread and returns the results. The round trips belong to that thread. */
    fun run(): String {
        try {
            recordReaderFormats()
            checkVirtualDisplay()
            checkGroupedStream()
            checkPrivateStream()
            checkSolidColorRenderer()
            checkYuvStream()
        } catch (failure: Exception) {
            report("run", StepResult(matches = false, buildJson("failure" to failure.toString())))
        }
        return results.toString()
    }

    /**
     * Records which buffer format a VirtualDisplay with a Presentation sends to each kind of ImageReader. For the
     * ImageReader read by the CPU, also records the color that it receives.
     */
    private fun recordReaderFormats() = listOf(
        ImageReaderOptions(PixelFormat.RGBA_8888, readByCpu = false),
        ImageReaderOptions(ImageFormat.PRIVATE, readByCpu = false),
        ImageReaderOptions(PixelFormat.RGBA_8888, readByCpu = true),
    ).forEach(::recordReaderFormat)

    private fun recordReaderFormat(options: ImageReaderOptions) {
        val reader = ImageReader.newInstance(
            metrics.widthPixels, metrics.heightPixels, options.format, IMAGES_HELD_AT_ONCE, options.usage,
        )
        val arrived = CountDownLatch(1)
        reader.setOnImageAvailableListener({ arrived.countDown() }, main)
        // A paused or stopped VirtualDisplay sends no more buffers, so the wait ends without an image.
        val ended = object : VirtualDisplay.Callback() {
            override fun onPaused() = arrived.countDown()
            override fun onStopped() = arrived.countDown()
        }
        val colorDisplay = showColor(reader.surface, Color.RED, ended)
        val evidence = buildJson("readerFormat" to options.format, "readerUsage" to options.usage.toHexFlags())
        arrived.await()
        try {
            reader.acquireNextImage().use { image ->
                evidence.put("delivered", image != null)
                image?.let { describeImage(it, options.readByCpu, evidence) }
            }
        } catch (failure: RuntimeException) {
            evidence.put("acquireError", failure.toString())
        }
        report("format", StepResult(matches = null, evidence))
        colorDisplay.hide()
        reader.close()
    }

    /**
     * Adds the formats of [image] and of its hardware buffer to [evidence]. With [readByCpu], also adds the color of
     * the pixel at the center of the crop rectangle.
     */
    private fun describeImage(image: Image, readByCpu: Boolean, evidence: JSONObject) {
        evidence.put("imageFormat", image.format)
            .put("buffer", image.hardwareBuffer?.use { it.toJson() } ?: JSONObject.NULL)
        if (readByCpu) {
            val plane = image.planes.first()
            val center = image.cropRect.centerY() * plane.rowStride + image.cropRect.centerX() * plane.pixelStride
            val rgba = plane.buffer.order(ByteOrder.BIG_ENDIAN).getInt(center)
            evidence.put("middle", rgba.rotateRight(Byte.SIZE_BITS).toHexColor())
        }
    }

    /**
     * Draws colors with a VirtualDisplay into a stream and checks each color on the pane. Then turns the VirtualDisplay
     * to the other orientation, first on the same Surface, then after a resize of the stream on the earlier Surface and
     * on the new one. Last, records what the stream receives while the display draws frames that are not latched.
     */
    private fun checkVirtualDisplay() {
        val (width, height) = metrics.widthPixels to metrics.heightPixels
        // The maximum size of the stream fits the buffers of the display in either orientation.
        val roundTrip = create(width.toUInt(), height.toUInt(), longestSide, longestSide, rgba = true)
        val colorDisplay = showColor(createSurface(roundTrip), Color.RED)
        listOf(NOTHING_SHOWN, Color.RED, Color.GREEN, Color.BLUE).zipWithNext().forEach { (before, color) ->
            colorDisplay.paint(color)
            report("color", latchColorChange(roundTrip, before, color))
        }
        runOnMain { colorDisplay.virtualDisplay.resize(height, width, metrics.densityDpi) }
        colorDisplay.paint(Color.RED)
        val turned = latchColorChange(roundTrip, Color.BLUE, Color.RED)
        turned.evidence.put("reached", hasDrawnSize(roundTrip, height, width))
        // The step only records whether the buffers on the same Surface change to the new size of the display.
        report("virtual-display-resize", turned.copy(matches = null))
        resize(roundTrip, height.toUInt(), width.toUInt(), longestSide, longestSide)
        val next = createSurface(roundTrip)
        colorDisplay.paint(Color.GREEN)
        report("earlier-surface-after-resize", latchColorChange(roundTrip, Color.RED, Color.GREEN))
        runOnMain { colorDisplay.virtualDisplay.surface = next }
        colorDisplay.paint(Color.BLUE)
        val moved = latchColorChange(roundTrip, Color.GREEN, Color.BLUE)
        report("new-surface-after-resize", moved.copy(matches = hasDrawnSize(roundTrip, height, width)))
        report("unlatched-virtual-display", StepResult(matches = null, recordUnlatchedFrames(roundTrip, colorDisplay)))
        colorDisplay.hide()
        destroy(roundTrip)
    }

    /**
     * Checks the latched frame drawn over black inside a group that moves the frame right by half the pane and fades
     * the frame.
     */
    private fun checkGroupedStream() {
        val (width, height) = metrics.widthPixels.toUInt() to metrics.heightPixels.toUInt()
        val roundTrip = create(width, height, width, height, rgba = true)
        val colorDisplay = showColor(createSurface(roundTrip), Color.RED)
        val latched = latchColorChange(roundTrip, NOTHING_SHOWN, Color.RED)
        val (outside, inside) = renderGrouped(roundTrip, FADED_OPACITY)
        val faded = Color.valueOf(encodeSrgb(FADED_OPACITY), 0f, 0f).toArgb()
        // The expected color and the color on the pane are each rounded to 8 bits once.
        val matches = outside == Color.BLACK && inside.isNearColor(faded, computeTolerance(roundings = 2))
        latched.evidence.put("outside", outside.toHexColor()).put("inside", inside.toHexColor())
        report("grouped-stream", latched.copy(matches = matches))
        colorDisplay.hide()
        destroy(roundTrip)
    }

    /** Checks whether a stream that accepts every buffer format receives frames of a VirtualDisplay. */
    private fun checkPrivateStream() {
        val roundTrip = create(displayWidth, displayHeight, longestSide, longestSide, rgba = false)
        val colorDisplay = showColor(createSurface(roundTrip), Color.RED)
        report("private-stream", latchColorChange(roundTrip, NOTHING_SHOWN, Color.RED))
        colorDisplay.hide()
        destroy(roundTrip)
    }

    /**
     * Draws one frame with [SolidColorRenderer] and latches it. Then draws more frames than any buffer queue has slots,
     * without a latch in between. Checks that the batch notifies the round trip once and that the next latch shows the
     * last color.
     */
    private fun checkSolidColorRenderer() {
        val (width, height) = metrics.widthPixels to metrics.heightPixels
        val roundTrip = create(width.toUInt(), height.toUInt(), width.toUInt(), height.toUInt(), rgba = true)
        val posted = BUFFER_SLOTS + 1
        val renderer = SolidColorRenderer(createSurface(roundTrip), width, height)
        renderer.draw(Color.CYAN)
        awaitWake(roundTrip)
        latch(roundTrip)
        val single = render(roundTrip)
        // A frame notifies the round trip at most once. Every notification after this count belongs to the batch.
        val singleWakes = countWakes(roundTrip)
        repeat(posted) { renderer.draw(Color.MAGENTA) }
        awaitWake(roundTrip)
        latch(roundTrip)
        val batch = render(roundTrip)
        renderer.close()
        val evidence = describeState(roundTrip)
        val batchWakes = countWakes(roundTrip) - singleWakes + destroyCountingWakes(roundTrip)
        evidence.put("posted", posted).put("single", single.toHexColor()).put("batch", batch.toHexColor())
            .put("singleWakes", singleWakes).put("batchWakes", batchWakes)
        // A frame that replaces a frame waiting to be latched does not notify the round trip, so a batch notifies once.
        val matches = singleWakes == 1L && batchWakes == 1L && single == Color.CYAN && batch == Color.MAGENTA
        report("unlatched-gpu-frames", StepResult(matches, evidence))
    }

    /**
     * Writes YUV frames with the CPU into a stream that accepts every buffer format. Checks the color of each frame on
     * the pane.
     */
    private fun checkYuvStream() {
        val roundTrip = create(displayWidth, displayHeight, longestSide, longestSide, rgba = false)
        val writer = ImageWriter.newInstance(createSurface(roundTrip), IMAGES_HELD_AT_ONCE, ImageFormat.YUV_420_888)
        for (color in listOf(Color.RED, Color.GREEN)) {
            val image = writer.dequeueInputImage()
            val hardwareBuffer = checkNotNull(image.hardwareBuffer) { "ImageWriter returned an image without a buffer" }
            val (bufferDescription, ycbcr) = hardwareBuffer.use { buffer ->
                buffer.toJson() to
                    encodeYcbcr(roundTrip, buffer, color)
            }
            fillYuvImage(image, ycbcr.codes)
            writer.queueInputImage(image)
            // The writer has queued only this frame since the last latch, so one latch shows it.
            latch(roundTrip)
            val result = compareColor(roundTrip, render(roundTrip), color, ycbcr.tolerance)
            result.evidence.put("buffer", bufferDescription).put("conversion", ycbcr.toJson())
            report("yuv-stream", result)
        }
        writer.close()
        destroy(roundTrip)
    }

    /** Writes [codes] into every sample of the Y, Cb and Cr planes of a YUV 4:2:0 [image]. */
    private fun fillYuvImage(image: Image, codes: List<Int>) {
        image.planes.zip(codes).forEachIndexed { index, (plane, code) ->
            val subsampling = if (index == 0) 1 else 2
            val bytes = plane.buffer
            for (row in 0 until (image.height + subsampling - 1) / subsampling) {
                for (column in 0 until (image.width + subsampling - 1) / subsampling) {
                    bytes.put(row * plane.rowStride + column * plane.pixelStride, code.toByte())
                }
            }
        }
    }

    /**
     * Latches and draws the arriving frames until the pane shows a color other than [before], the color shown before
     * the paint. Compares that color with [expected].
     */
    private fun latchColorChange(roundTrip: Long, before: Int, expected: Int): StepResult {
        val observed = generateSequence { latch(roundTrip); render(roundTrip) }.first { it != before }
        return compareColor(roundTrip, observed, expected)
    }

    /** Compares [observed] with [expected], allowing each color component to differ by [tolerance] 8-bit steps. */
    private fun compareColor(roundTrip: Long, observed: Int, expected: Int, tolerance: Int = 0) = StepResult(
        observed.isNearColor(expected, tolerance),
        describeState(roundTrip).put("expected", expected.toHexColor()).put("observed", observed.toHexColor()),
    )

    /**
     * Lets the view of [colorDisplay] draw a new color every frame, more frames than any buffer queue has slots,
     * without a latch in between. Then waits for the notification of the stream, latches the waiting frame and
     * then a frame drawn after it.
     */
    private fun recordUnlatchedFrames(roundTrip: Long, colorDisplay: ColorDisplay): JSONObject {
        runOnMain {
            colorDisplay.view.changesEveryFrame = true
            colorDisplay.view.invalidate()
        }
        latch(roundTrip)
        val draws = BUFFER_SLOTS + 1
        val drawn = CountDownLatch(draws)
        runOnMain { colorDisplay.view.onFrameDrawn = drawn::countDown }
        drawn.await()
        awaitWake(roundTrip)
        latch(roundTrip)
        latch(roundTrip)
        return describeState(roundTrip).put("drawsWhileUnlatched", draws)
    }

    private fun hasDrawnSize(roundTrip: Long, width: Int, height: Int) =
        readDrawnSize(roundTrip).contentEquals(longArrayOf(width.toLong(), height.toLong()))

    private fun describeState(roundTrip: Long) =
        buildJson("drawnSize" to readDrawnSize(roundTrip), "wakes" to countWakes(roundTrip))

    private fun report(step: String, result: StepResult) {
        results.appendLine(buildJson("step" to step, "matches" to result.matches, "evidence" to result.evidence))
    }

    private fun <T> runOnMain(work: () -> T): T = FutureTask { work() }.also { main.post(it) }.get()

    /**
     * Shows [color] on a new VirtualDisplay. The display draws into [surface] and calls [callback] when the display
     * pauses, resumes or stops.
     */
    private fun showColor(surface: Surface, color: Int, callback: VirtualDisplay.Callback? = null) = runOnMain {
        val virtualDisplay = getSystemService(DisplayManager::class.java).createVirtualDisplay(
            "lavashell-round-trip", metrics.widthPixels, metrics.heightPixels, metrics.densityDpi, surface,
            DisplayManager.VIRTUAL_DISPLAY_FLAG_OWN_CONTENT_ONLY, callback, main,
        )
        val presentation = Presentation(this, virtualDisplay.display)
        // The window animation is off, so that no frame blends the new content with the content shown before.
        presentation.window?.setWindowAnimations(Resources.ID_NULL)
        val view = SolidColorView(presentation.context, color)
        presentation.setContentView(view)
        presentation.show()
        ColorDisplay(virtualDisplay, presentation, view)
    }

    private fun ColorDisplay.paint(color: Int) = runOnMain {
        view.color = color
        view.invalidate()
    }

    private fun ColorDisplay.hide() = runOnMain {
        presentation.dismiss()
        virtualDisplay.release()
    }

    companion object {
        /**
         * A buffer queue in AOSP has at most BufferQueueDefs::NUM_BUFFER_SLOTS buffers:
         * https://android.googlesource.com/platform/frameworks/native/+/main/libs/ui/include/ui/BufferQueueDefs.h
         */
        private const val BUFFER_SLOTS = 64

        /** The color that the pane shows from a VirtualDisplay before the Presentation on that display has drawn. */
        private const val NOTHING_SHOWN = Color.BLACK

        /** The opacity of the group in the step grouped-stream, which halves the light of the latched frame. */
        private const val FADED_OPACITY = 0.5f

        /** Every step holds at most one image of an ImageReader or an ImageWriter at a time. */
        private const val IMAGES_HELD_AT_ONCE = 1

        init {
            System.loadLibrary("surface_stream_round_trip")
        }

        // Every native method is called on the thread of `run`, because the native library stores the round trips per
        // thread. A failed native method throws a RuntimeException. A JvmName keeps the JNI name of a method that takes
        // a UInt, which Kotlin would otherwise change.
        @JvmStatic
        @JvmName("create")
        external fun create(width: UInt, height: UInt, maxWidth: UInt, maxHeight: UInt, rgba: Boolean): Long

        @JvmStatic external fun createSurface(roundTrip: Long): Surface

        /** Blocks until the stream latches a frame that it has not latched before. */
        @JvmStatic external fun latch(roundTrip: Long)

        /** Blocks until the stream notifies the round trip of a frame. */
        @JvmStatic external fun awaitWake(roundTrip: Long)

        /** Draws the latched frame over the pane and returns the color of the pixel at the center of the pane. */
        @JvmStatic external fun render(roundTrip: Long): Int

        /**
         * Draws the latched frame over black inside a group that moves the frame right by half the width of the pane
         * and fades the frame to [opacity]. Returns the colors of the pixels at the centers of the left and of the
         * right half of the pane.
         */
        @JvmStatic external fun renderGrouped(roundTrip: Long, opacity: Float): IntArray

        /** The width and the height of the frame latched last, or null before the first latch. */
        @JvmStatic external fun readDrawnSize(roundTrip: Long): LongArray?

        /** How often the round trip has received a notification of a frame from the stream. */
        @JvmStatic external fun countWakes(roundTrip: Long): Long

        @JvmStatic
        @JvmName("resize")
        external fun resize(roundTrip: Long, width: UInt, height: UInt, maxWidth: UInt, maxHeight: UInt)

        @JvmStatic external fun destroy(roundTrip: Long)

        /**
         * Destroys the round trip and returns how many notifications of a frame the stream sent that no call of
         * [awaitWake] or [latch] received.
         */
        @JvmStatic external fun destroyCountingWakes(roundTrip: Long): Long

        /**
         * Encodes [color], a color int of android.graphics.Color, in YCbCr with the model and range that the Vulkan
         * device of the renderer in [roundTrip] suggests for [buffer], the same two with which the renderer samples
         * [buffer]. [buffer] stays open until the call returns, because the native method reads the hardware buffer
         * from it.
         */
        @JvmStatic external fun encodeYcbcr(roundTrip: Long, buffer: HardwareBuffer, color: Int): YcbcrConversion
    }
}

/**
 * The result of one step. [matches] says whether the step found the color it expected on the pane, or is null for a
 * step that only records. [evidence] contains what the step observed.
 */
private data class StepResult(val matches: Boolean?, val evidence: JSONObject)

private data class ImageReaderOptions(val format: Int, val readByCpu: Boolean) {
    val usage
        get() = HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE or (if (readByCpu) HardwareBuffer.USAGE_CPU_READ_OFTEN else 0L)
}

/** A private VirtualDisplay that shows a [SolidColorView] in a Presentation. */
private data class ColorDisplay(
    val virtualDisplay: VirtualDisplay,
    val presentation: Presentation,
    val view: SolidColorView,
)

private val COLOR_COMPONENTS = listOf<(Int) -> Int>(Color::alpha, Color::red, Color::green, Color::blue)

private fun Int.isNearColor(expected: Int, tolerance: Int) =
    COLOR_COMPONENTS.all { component -> abs(component(this) - component(expected)) <= tolerance }

/**
 * Each rounding to 8 bits moves a color component by at most half an 8-bit step, so [roundings] roundings move the
 * component by at most this many whole steps.
 */
private fun computeTolerance(roundings: Int) = (roundings + 1) / 2

/** The renderer blends in linear light, so the expected color of a faded frame is the sRGB encoding of [linear]. */
private fun encodeSrgb(linear: Float) =
    (ColorSpace.get(ColorSpace.Named.SRGB) as ColorSpace.Rgb).oetf.applyAsDouble(linear.toDouble()).toFloat()

private fun HardwareBuffer.toJson() = buildJson("format" to format, "usage" to usage.toHexFlags())

private fun Int.toHexColor() = "#%08x".format(this)

private fun Long.toHexFlags() = "%x".format(this)

internal fun buildJson(vararg fields: Pair<String, Any?>) = JSONObject(mapOf(*fields))
