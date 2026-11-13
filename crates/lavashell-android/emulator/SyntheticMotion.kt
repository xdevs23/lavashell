// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent

/**
 * A motion event of one pointer of [toolType] from a device of [source]. [buttonState] contains the buttons that are
 * down after the event, and [actionButton] is the button that a button action presses or releases.
 */
internal data class SyntheticMotion(
    val action: Int,
    val toolType: Int,
    val source: Int,
    val buttonState: Int = 0,
    val actionButton: Int = 0,
) {
    /** A new MotionEvent at [x], [y] in pixels. The caller recycles it. */
    fun obtain(x: Float, y: Float): MotionEvent {
        val now = SystemClock.uptimeMillis()
        val properties = MotionEvent.PointerProperties().apply { this.toolType = this@SyntheticMotion.toolType }
        val coords = MotionEvent.PointerCoords().apply {
            this.x = x
            this.y = y
        }
        return MotionEvent.obtain(
            now, now, action, 1, arrayOf(properties), arrayOf(coords), 0, buttonState, 1f, 1f, 0, 0, source, 0,
        ).also { setActionButton.invoke(it, actionButton) }
    }

    companion object {
        /** A touch down, move and up of a finger, then a click of the primary mouse button while the mouse hovers. */
        val TOUCH_AND_CLICK = listOf(
            touch(MotionEvent.ACTION_DOWN),
            touch(MotionEvent.ACTION_MOVE),
            touch(MotionEvent.ACTION_UP),
            mouse(MotionEvent.ACTION_HOVER_ENTER),
            mouse(
                MotionEvent.ACTION_BUTTON_PRESS,
                buttonState = MotionEvent.BUTTON_PRIMARY,
                actionButton = MotionEvent.BUTTON_PRIMARY,
            ),
            mouse(MotionEvent.ACTION_BUTTON_RELEASE, actionButton = MotionEvent.BUTTON_PRIMARY),
            mouse(MotionEvent.ACTION_HOVER_EXIT),
        )

        /** MotionEvent.obtain sets no action button, and MotionEvent offers its setter only as a test API. */
        private val setActionButton = MotionEvent::class.java.getMethod("setActionButton", Int::class.java)

        private fun touch(action: Int) =
            SyntheticMotion(action, MotionEvent.TOOL_TYPE_FINGER, InputDevice.SOURCE_TOUCHSCREEN)

        private fun mouse(action: Int, buttonState: Int = 0, actionButton: Int = 0) =
            SyntheticMotion(action, MotionEvent.TOOL_TYPE_MOUSE, InputDevice.SOURCE_MOUSE, buttonState, actionButton)
    }
}
