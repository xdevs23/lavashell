// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.graphics.HardwareRenderer
import android.graphics.RenderNode
import android.view.Surface

/**
 * Draws frames filled with one color into [surface] on the GPU, with the same HardwareRenderer that draws the views of
 * an app. Each frame is a list of drawing commands, recorded into a RenderNode, that fills the whole frame with one
 * color. The HardwareRenderer draws that list into the next buffer of [surface] and queues the buffer.
 */
internal class SolidColorRenderer(surface: Surface, private val width: Int, private val height: Int) : AutoCloseable {
    private val frame = RenderNode("lavashell-round-trip").apply { setPosition(0, 0, width, height) }
    private val renderer = HardwareRenderer().apply {
        setContentRoot(frame)
        setSurface(surface)
    }

    /** Draws one frame filled with [color]. Returns once its buffer is in the queue of the surface. */
    fun draw(color: Int) {
        frame.beginRecording(width, height).drawColor(color)
        frame.endRecording()
        val synced = renderer.createRenderRequest().setWaitForPresent(true).syncAndDraw()
        check(synced == HardwareRenderer.SYNC_OK) { "HardwareRenderer did not queue the frame: $synced" }
    }

    override fun close() = renderer.destroy()
}
