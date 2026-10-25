// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

/**
 * The YCbCr codes of one test color for one hardware buffer, created by the native method
 * `StreamRoundTripActivity.encodeYcbcr`. [tolerance] is how many 8-bit steps each component of the color drawn from the
 * buffer may differ from the test color. The fields after it are the format properties returned by Vulkan for the
 * buffer, each as its raw Vulkan enum or flag value. [externalFormat] contains the bits of an unsigned 64-bit value.
 */
data class YcbcrConversion(
    val y: Int,
    val cb: Int,
    val cr: Int,
    val tolerance: Int,
    val format: Int,
    val externalFormat: Long,
    val formatFeatures: Long,
    val model: Int,
    val range: Int,
    val xChromaOffset: Int,
    val yChromaOffset: Int,
) {
    val codes get() = listOf(y, cb, cr)

    fun toJson() = buildJson(
        "ycbcr" to codes,
        "tolerance" to tolerance,
        "vulkan" to mapOf(
            "format" to format,
            "externalFormat" to externalFormat.toULong().toString(),
            "formatFeatures" to formatFeatures,
            "model" to model,
            "range" to range,
            "xChromaOffset" to xChromaOffset,
            "yChromaOffset" to yChromaOffset,
        ),
    )
}
