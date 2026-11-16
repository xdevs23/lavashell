// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.app.Activity

class StreamRoundTripActivity : Activity() {
    fun run(): String {
        val report = Report()
        try {
            StreamRoundTripScenarios(this, MainThread(), report).runAll()
        } catch (failure: Exception) {
            report.add(Step.Run, StepResult(Verdict.Mismatched, RunFailure(failure)))
        }
        return report.toString()
    }
}
