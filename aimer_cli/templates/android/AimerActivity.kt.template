package com.aimer

import android.app.NativeActivity
import android.content.Context
import android.os.Bundle
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.ViewGroup
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.EditText

/**
 * Thin wrapper around [NativeActivity] that gives the framework a working
 * software keyboard with full IME support (Chinese / Japanese / Korean, emoji,
 * autocorrect, ...).
 *
 * A bare `NativeActivity` renders into a native surface that has no
 * [android.view.inputmethod.InputConnection], so the system IME has nowhere to
 * deliver composed text and CJK input is silently dropped. To fix this we add a
 * hidden, focusable [EditText] on top of the native surface and let the IME
 * compose into it. A composing-aware [TextWatcher] forwards only finalized /
 * committed text back into Rust through the [nativeInsertText] JNI bridge.
 *
 * Rust calls [showKeyboard] / [hideKeyboard] via JNI when a text field gains or
 * loses focus.
 */
class AimerActivity : NativeActivity() {

    private var inputView: EditText? = null
    private var suppressWatcher = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        runOnUiThread(::setupInputView)
    }

    private fun setupInputView() {
        val view = EditText(this).apply {
            // Effectively invisible but still focusable so it can own the IME session.
            alpha = 0f
            isFocusable = true
            isFocusableInTouchMode = true
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI
        }

        view.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}

            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}

            override fun afterTextChanged(s: Editable) {
                if (suppressWatcher) {
                    return
                }

                // While the IME is composing (e.g. Pinyin candidates), the text
                // carries a "composing" span. Wait for the user to commit before
                // forwarding anything so partial composition is never inserted.
                val composeStart = BaseInputConnection.getComposingSpanStart(s)
                val composeEnd = BaseInputConnection.getComposingSpanEnd(s)
                if (composeStart != -1 && composeEnd != -1 && composeEnd > composeStart) {
                    return
                }

                val text = s.toString()
                if (text.length > PLACEHOLDER.length) {
                    // Everything past the sentinel is freshly committed text.
                    nativeInsertText(text.substring(PLACEHOLDER.length))
                    resetPlaceholder()
                } else if (text.isEmpty()) {
                    // The sentinel itself was deleted -> backspace past the start.
                    nativeBackspace()
                    resetPlaceholder()
                }
            }
        })

        addContentView(view, ViewGroup.LayoutParams(1, 1))
        inputView = view
        resetPlaceholder()
    }

    private fun resetPlaceholder() {
        val view = inputView ?: return
        suppressWatcher = true
        view.setText(PLACEHOLDER)
        view.setSelection(PLACEHOLDER.length)
        suppressWatcher = false
    }

    /** Called from Rust via JNI when a text field gains focus. */
    @Suppress("unused")
    fun showKeyboard() {
        runOnUiThread {
            val view = inputView ?: return@runOnUiThread
            resetPlaceholder()
            view.requestFocus()
            val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
            // No flags: `SHOW_IMPLICIT` is deprecated and, since Android 13, is
            // ignored outright by some IMEs. The view was just focused above, so
            // an explicit request is exactly what is wanted here.
            imm?.showSoftInput(view, 0)
        }
    }

    /** Called from Rust via JNI when the focused text field is dismissed. */
    @Suppress("unused")
    fun hideKeyboard() {
        runOnUiThread {
            val view = inputView ?: return@runOnUiThread
            val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
            imm?.hideSoftInputFromWindow(view.windowToken, 0)
        }
    }

    companion object {
        /**
         * One-character sentinel kept in the hidden [EditText] so the backspace
         * key always has something to delete (and therefore keeps firing) even
         * when the logical field is empty.
         */
        private const val PLACEHOLDER = " "

        /**
         * Implemented in Rust (`Java_com_aimer_AimerActivity_nativeInsertText`).
         *
         * `@JvmStatic` is required: the Rust symbol is bound to a *static* native
         * method on `com.aimer.AimerActivity`, which a plain companion member
         * would not produce.
         */
        @JvmStatic
        external fun nativeInsertText(text: String)

        /** Implemented in Rust (`Java_com_aimer_AimerActivity_nativeBackspace`). */
        @JvmStatic
        external fun nativeBackspace()
    }
}
