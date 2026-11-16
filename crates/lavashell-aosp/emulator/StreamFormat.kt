// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.graphics.ImageFormat
import android.graphics.PixelFormat

internal enum class StreamFormat(val id: Int) {
    Private(ImageFormat.PRIVATE),
    Rgba8888(PixelFormat.RGBA_8888),
}
