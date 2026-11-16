// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.graphics.HardwareRenderer
import android.graphics.RenderNode
import android.view.Surface

internal class SolidColorRenderer(surface: Surface, private val size: PixelSize) : AutoCloseable {
    private val frame = RenderNode("lavashell-round-trip").apply { setPosition(0, 0, size.width, size.height) }
    private val renderer = HardwareRenderer().apply {
        setContentRoot(frame)
        setSurface(surface)
    }

    fun draw(color: Int) {
        frame.beginRecording(size.width, size.height).drawColor(color)
        frame.endRecording()
        val synced = renderer.createRenderRequest().setWaitForPresent(true).syncAndDraw()
        check(synced == HardwareRenderer.SYNC_OK) { "HardwareRenderer did not queue the frame: $synced" }
    }

    override fun close() = renderer.destroy()
}
