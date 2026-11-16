// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import org.json.JSONObject

internal enum class Step(val id: String) {
    Run("run"),
    Format("format"),
    Color("color"),
    VirtualDisplayResize("virtual-display-resize"),
    EarlierSurfaceAfterResize("earlier-surface-after-resize"),
    NewSurfaceAfterResize("new-surface-after-resize"),
    UnlatchedVirtualDisplay("unlatched-virtual-display"),
    GroupedStream("grouped-stream"),
    PrivateStream("private-stream"),
    UnlatchedGpuFrames("unlatched-gpu-frames"),
    YuvStream("yuv-stream"),
}

internal sealed class Verdict(val matches: Boolean?) {
    object Matched : Verdict(true)

    object Mismatched : Verdict(false)

    object Recorded : Verdict(null)
}

internal fun verdictOf(matches: Boolean): Verdict = if (matches) Verdict.Matched else Verdict.Mismatched

internal interface Evidence {
    fun toJson(): JSONObject
}

internal data class StepResult<out E : Evidence>(val verdict: Verdict, val evidence: E)

internal class Report {
    private val lines = StringBuilder()

    fun add(step: Step, result: StepResult<Evidence>) {
        lines.appendLine(
            JSONObject(
                mapOf("step" to step.id, "matches" to result.verdict.matches, "evidence" to result.evidence.toJson()),
            ),
        )
    }

    override fun toString() = lines.toString()
}
