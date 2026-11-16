// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.app.Activity
import android.graphics.Color
import android.graphics.ColorSpace
import android.graphics.ImageFormat
import android.graphics.PixelFormat
import android.hardware.HardwareBuffer
import android.hardware.display.VirtualDisplay
import android.media.Image
import android.media.ImageReader
import android.media.ImageWriter
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import kotlin.math.abs
import kotlin.math.max

internal data class ImageReaderOptions(val format: Int, val readByCpu: Boolean) {
    val usage
        get() = HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE or (if (readByCpu) HardwareBuffer.USAGE_CPU_READ_OFTEN else 0L)
}

internal class StreamRoundTripScenarios(
    private val activity: Activity,
    private val main: MainThread,
    private val report: Report,
) {
    private val metrics = activity.resources.displayMetrics
    private val displaySize = PixelSize(metrics.widthPixels, metrics.heightPixels)
    private val longestSide = max(metrics.widthPixels, metrics.heightPixels)
    private val longestSquare = PixelSize(longestSide, longestSide)

    fun runAll() {
        recordReaderFormats()
        checkVirtualDisplay()
        checkGroupedStream()
        checkPrivateStream()
        checkSolidColorRenderer()
        checkYuvStream()
    }

    private fun recordReaderFormats() = listOf(
        ImageReaderOptions(PixelFormat.RGBA_8888, readByCpu = false),
        ImageReaderOptions(ImageFormat.PRIVATE, readByCpu = false),
        ImageReaderOptions(PixelFormat.RGBA_8888, readByCpu = true),
    ).forEach(::recordReaderFormat)

    private fun recordReaderFormat(options: ImageReaderOptions) {
        ImageReader.newInstance(
            displaySize.width, displaySize.height, options.format, IMAGES_HELD_AT_ONCE, options.usage,
        ).use { reader ->
            val arrived = CountDownLatch(1)
            reader.setOnImageAvailableListener({ arrived.countDown() }, main.handler)
            val ended = object : VirtualDisplay.Callback() {
                override fun onPaused() = arrived.countDown()
                override fun onStopped() = arrived.countDown()
            }
            ColorDisplay.show(activity, main, reader.surface, Color.RED, ended).use {
                arrived.await()
                val evidence = ReaderFormatEvidence(options, acquire(reader, options))
                report.add(Step.Format, StepResult(Verdict.Recorded, evidence))
            }
        }
    }

    private fun acquire(reader: ImageReader, options: ImageReaderOptions): AcquiredImage = try {
        reader.acquireNextImage().use { image ->
            if (image == null) {
                AcquiredImage.NotDelivered
            } else {
                AcquiredImage.Delivered(
                    imageFormat = image.format,
                    buffer = image.hardwareBuffer?.use { it.describe() },
                    middle = if (options.readByCpu) readMiddle(image) else null,
                )
            }
        }
    } catch (failure: RuntimeException) {
        AcquiredImage.Failed(failure.toString())
    }

    private fun readMiddle(image: Image): Int {
        val plane = image.planes.first()
        val center = image.cropRect.centerY() * plane.rowStride + image.cropRect.centerX() * plane.pixelStride
        return plane.buffer.order(ByteOrder.BIG_ENDIAN).getInt(center).rotateRight(Byte.SIZE_BITS)
    }

    private fun checkVirtualDisplay() {
        val (width, height) = displaySize
        StreamRoundTrip(BufferSizes(displaySize, longestSquare), StreamFormat.Rgba8888).use { roundTrip ->
            ColorDisplay.show(activity, main, roundTrip.surface(), Color.RED).use { colorDisplay ->
                listOf(NOTHING_SHOWN, Color.RED, Color.GREEN, Color.BLUE).zipWithNext().forEach { (before, color) ->
                    colorDisplay.paint(color)
                    report.add(Step.Color, latchColorChange(roundTrip, before, color))
                }
                colorDisplay.resize(height, width, metrics.densityDpi)
                colorDisplay.paint(Color.RED)
                val turned = latchColorChange(roundTrip, Color.BLUE, Color.RED)
                val reached = ColorExtra.Reached(hasDrawnSize(roundTrip, height, width))
                report.add(
                    Step.VirtualDisplayResize,
                    StepResult(Verdict.Recorded, turned.evidence.copy(extra = reached)),
                )
                roundTrip.resize(BufferSizes(PixelSize(height, width), longestSquare))
                val next = roundTrip.surface()
                colorDisplay.paint(Color.GREEN)
                report.add(Step.EarlierSurfaceAfterResize, latchColorChange(roundTrip, Color.RED, Color.GREEN))
                colorDisplay.moveTo(next)
                colorDisplay.paint(Color.BLUE)
                val moved = latchColorChange(roundTrip, Color.GREEN, Color.BLUE)
                val movedToNewSize = verdictOf(hasDrawnSize(roundTrip, height, width))
                report.add(Step.NewSurfaceAfterResize, moved.copy(verdict = movedToNewSize))
                val unlatched = recordUnlatchedFrames(roundTrip, colorDisplay)
                report.add(Step.UnlatchedVirtualDisplay, StepResult(Verdict.Recorded, unlatched))
            }
        }
    }

    private fun checkGroupedStream() {
        StreamRoundTrip(BufferSizes(displaySize, displaySize), StreamFormat.Rgba8888).use { roundTrip ->
            ColorDisplay.show(activity, main, roundTrip.surface(), Color.RED).use {
                val latched = latchColorChange(roundTrip, NOTHING_SHOWN, Color.RED)
                val (outside, inside) = roundTrip.renderGrouped(FADED_OPACITY)
                val faded = Color.valueOf(encodeSrgb(FADED_OPACITY), 0f, 0f).toArgb()
                val matches = outside == Color.BLACK && inside.isNearColor(faded, computeTolerance(roundings = 2))
                report.add(
                    Step.GroupedStream,
                    StepResult(verdictOf(matches), latched.evidence.copy(extra = ColorExtra.Grouped(outside, inside))),
                )
            }
        }
    }

    private fun checkPrivateStream() {
        StreamRoundTrip(BufferSizes(displaySize, longestSquare), StreamFormat.Private).use { roundTrip ->
            ColorDisplay.show(activity, main, roundTrip.surface(), Color.RED).use {
                report.add(Step.PrivateStream, latchColorChange(roundTrip, NOTHING_SHOWN, Color.RED))
            }
        }
    }

    private fun checkSolidColorRenderer() {
        val posted = BUFFER_QUEUE_SLOTS + 1
        StreamRoundTrip(BufferSizes(displaySize, displaySize), StreamFormat.Rgba8888).use { roundTrip ->
            val (single, singleWakes, batch) = SolidColorRenderer(roundTrip.surface(), displaySize).use { renderer ->
                renderer.draw(Color.CYAN)
                roundTrip.awaitWake()
                roundTrip.latch()
                val firstColor = roundTrip.render()
                val wakesAfterFirst = roundTrip.state().wakes
                repeat(posted) { renderer.draw(Color.MAGENTA) }
                roundTrip.awaitWake()
                roundTrip.latch()
                Triple(firstColor, wakesAfterFirst, roundTrip.render())
            }
            val state = roundTrip.state()
            roundTrip.close()
            val batchWakes = state.wakes - singleWakes + roundTrip.unreceivedWakes()
            val matches = singleWakes == 1L && batchWakes == 1L && single == Color.CYAN && batch == Color.MAGENTA
            report.add(
                Step.UnlatchedGpuFrames,
                StepResult(
                    verdictOf(matches),
                    UnlatchedGpuFramesEvidence(state, posted, single, batch, singleWakes, batchWakes),
                ),
            )
        }
    }

    private fun checkYuvStream() {
        StreamRoundTrip(BufferSizes(displaySize, longestSquare), StreamFormat.Private).use { roundTrip ->
            ImageWriter.newInstance(roundTrip.surface(), IMAGES_HELD_AT_ONCE, ImageFormat.YUV_420_888).use { writer ->
                for (color in listOf(Color.RED, Color.GREEN)) {
                    val image = writer.dequeueInputImage()
                    val hardwareBuffer = checkNotNull(image.hardwareBuffer) {
                        "ImageWriter returned an image without a buffer"
                    }
                    val (bufferDescription, ycbcr) = hardwareBuffer.use { buffer ->
                        buffer.describe() to roundTrip.encodeYcbcr(buffer, color)
                    }
                    fillYuvImage(image, ycbcr.codes.toList())
                    writer.queueInputImage(image)
                    roundTrip.latch()
                    val result = compareColor(roundTrip, roundTrip.render(), color, ycbcr.tolerance)
                    report.add(
                        Step.YuvStream,
                        result.copy(evidence = result.evidence.copy(extra = ColorExtra.Yuv(bufferDescription, ycbcr))),
                    )
                }
            }
        }
    }

    private fun fillYuvImage(image: Image, codes: List<Int>) {
        image.planes.zip(codes).forEachIndexed { index, (plane, code) ->
            val subsampling = if (index == LUMA_PLANE) NO_SUBSAMPLING else CHROMA_SUBSAMPLING
            val bytes = plane.buffer
            for (row in 0 until (image.height + subsampling - 1) / subsampling) {
                for (column in 0 until (image.width + subsampling - 1) / subsampling) {
                    bytes.put(row * plane.rowStride + column * plane.pixelStride, code.toByte())
                }
            }
        }
    }

    private fun latchColorChange(roundTrip: StreamRoundTrip, before: Int, expected: Int): StepResult<ColorEvidence> {
        val observed = generateSequence { roundTrip.latch(); roundTrip.render() }.first { it != before }
        return compareColor(roundTrip, observed, expected)
    }

    private fun compareColor(
        roundTrip: StreamRoundTrip,
        observed: Int,
        expected: Int,
        tolerance: Int = 0,
    ) = StepResult(
        verdictOf(observed.isNearColor(expected, tolerance)),
        ColorEvidence(roundTrip.state(), expected, observed),
    )

    private fun recordUnlatchedFrames(
        roundTrip: StreamRoundTrip,
        colorDisplay: ColorDisplay,
    ): UnlatchedFramesEvidence {
        colorDisplay.changeEveryFrame()
        roundTrip.latch()
        val draws = BUFFER_QUEUE_SLOTS + 1
        val drawn = CountDownLatch(draws)
        colorDisplay.onFrameDrawn(drawn::countDown)
        drawn.await()
        roundTrip.awaitWake()
        roundTrip.latch()
        roundTrip.latch()
        return UnlatchedFramesEvidence(roundTrip.state(), draws)
    }

    private fun hasDrawnSize(roundTrip: StreamRoundTrip, width: Int, height: Int) =
        roundTrip.state().drawnSize == PixelSize(width, height)

    private companion object {
        const val BUFFER_QUEUE_SLOTS = 64

        const val NOTHING_SHOWN = Color.BLACK

        const val FADED_OPACITY = 0.5f

        const val IMAGES_HELD_AT_ONCE = 1

        const val LUMA_PLANE = 0

        const val NO_SUBSAMPLING = 1

        const val CHROMA_SUBSAMPLING = 2
    }
}

private val COLOR_COMPONENTS = listOf<(Int) -> Int>(Color::alpha, Color::red, Color::green, Color::blue)

private fun Int.isNearColor(expected: Int, tolerance: Int) =
    COLOR_COMPONENTS.all { component -> abs(component(this) - component(expected)) <= tolerance }

private fun computeTolerance(roundings: Int) = (roundings + 1) / 2

private fun encodeSrgb(linear: Float): Float {
    val srgb = ColorSpace.get(ColorSpace.Named.SRGB)
    check(srgb is ColorSpace.Rgb) { "the sRGB color space is not an RGB color space: $srgb" }
    return srgb.oetf.applyAsDouble(linear.toDouble()).toFloat()
}
