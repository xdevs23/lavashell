// SPDX-License-Identifier: GPL-3.0-only
package org.lavashell.surfacestream

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.os.Bundle

/** Runs the steps of [StreamRoundTripActivity] and finishes with their results, which `am instrument -w` prints. */
class StreamRoundTripRunner : Instrumentation() {
    override fun onCreate(arguments: Bundle?) {
        super.onCreate(arguments)
        start()
    }

    override fun onStart() {
        val intent = Intent(targetContext, StreamRoundTripActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val activity = startActivitySync(intent) as StreamRoundTripActivity
        val results = activity.run()
        activity.finish()
        finish(Activity.RESULT_OK, Bundle().apply { putString(REPORT_KEY_STREAMRESULT, results) })
    }
}
