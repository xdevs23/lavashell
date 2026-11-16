// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import org.json.JSONObject

internal class YcbcrConversion(val codes: IntArray, val tolerance: Int, private val vulkan: String) {
    fun toJson() = JSONObject(mapOf("ycbcr" to codes.toList(), "tolerance" to tolerance, "vulkan" to vulkan))
}
