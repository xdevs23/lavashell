// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.os.Handler
import android.os.Looper
import java.util.concurrent.FutureTask

internal class MainThread {
    val handler = Handler(Looper.getMainLooper())

    fun <T> run(work: () -> T): T = FutureTask { work() }.also { handler.post(it) }.get()
}
