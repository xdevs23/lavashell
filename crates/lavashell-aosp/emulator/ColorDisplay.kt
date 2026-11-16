// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.app.Activity
import android.app.Presentation
import android.content.res.Resources
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.view.Surface

internal class ColorDisplay private constructor(
    private val main: MainThread,
    private val virtualDisplay: VirtualDisplay,
    private val presentation: Presentation,
    private val view: SolidColorView,
) : AutoCloseable {
    fun paint(color: Int) = main.run {
        view.color = color
        view.invalidate()
    }

    fun resize(width: Int, height: Int, densityDpi: Int) = main.run { virtualDisplay.resize(width, height, densityDpi) }

    fun moveTo(surface: Surface) = main.run { virtualDisplay.surface = surface }

    fun changeEveryFrame() = main.run {
        view.changesEveryFrame = true
        view.invalidate()
    }

    fun onFrameDrawn(callback: () -> Unit) = main.run { view.onFrameDrawn = callback }

    override fun close() = main.run {
        presentation.dismiss()
        virtualDisplay.release()
    }

    companion object {
        fun show(
            activity: Activity,
            main: MainThread,
            surface: Surface,
            color: Int,
            callback: VirtualDisplay.Callback? = null,
        ) = main.run {
            val metrics = activity.resources.displayMetrics
            val virtualDisplay = activity.getSystemService(DisplayManager::class.java).createVirtualDisplay(
                "lavashell-round-trip", metrics.widthPixels, metrics.heightPixels, metrics.densityDpi, surface,
                DisplayManager.VIRTUAL_DISPLAY_FLAG_OWN_CONTENT_ONLY, callback, main.handler,
            )
            val presentation = Presentation(activity, virtualDisplay.display)
            presentation.window?.setWindowAnimations(Resources.ID_NULL)
            val view = SolidColorView(presentation.context, color)
            presentation.setContentView(view)
            presentation.show()
            ColorDisplay(main, virtualDisplay, presentation, view)
        }
    }
}
