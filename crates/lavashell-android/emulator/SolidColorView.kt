// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.content.Context
import android.graphics.Canvas
import android.view.View

private const val LOWEST_RGB_BITS = 0x00010101

/** A view that draws [color] over its whole area. */
internal class SolidColorView(context: Context, var color: Int) : View(context) {
    /** Makes every frame flip the lowest bit of red, green and blue, so the view never draws the same frame twice. */
    var changesEveryFrame = false
    var onFrameDrawn: () -> Unit = {}

    override fun onDraw(canvas: Canvas) {
        if (changesEveryFrame) {
            color = color xor LOWEST_RGB_BITS
            postInvalidateOnAnimation()
        }
        onFrameDrawn()
        canvas.drawColor(color)
    }
}
