// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

internal data class PixelSize(val width: Int, val height: Int) {
    init {
        require(width > 0 && height > 0) { "a size has positive lengths, not $width x $height" }
    }
}

internal data class BufferSizes(val default: PixelSize, val max: PixelSize) {
    init {
        require(default.width <= max.width && default.height <= max.height) {
            "the default size $default exceeds the maximum $max"
        }
    }
}
