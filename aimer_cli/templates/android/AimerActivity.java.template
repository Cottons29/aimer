package com.aimer;

import android.app.NativeActivity;
import android.content.Context;
import android.os.Bundle;
import android.text.Editable;
import android.text.InputType;
import android.text.TextWatcher;
import android.view.ViewGroup;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;

/**
 * Thin wrapper around {@link NativeActivity} that gives the framework a working
 * software keyboard with full IME support (Chinese / Japanese / Korean, emoji,
 * autocorrect, ...).
 *
 * <p>A bare {@code NativeActivity} renders into a native surface that has no
 * {@link android.view.inputmethod.InputConnection}, so the system IME has nowhere
 * to deliver composed text and CJK input is silently dropped. To fix this we add a
 * hidden, focusable {@link EditText} on top of the native surface and let the IME
 * compose into it. A composing-aware {@link TextWatcher} forwards only finalized /
 * committed text back into Rust through the {@code nativeInsertText} JNI bridge.
 *
 * <p>Rust calls {@link #showKeyboard()} / {@link #hideKeyboard()} via JNI when a
 * text field gains or loses focus.
 */
public class AimerActivity extends NativeActivity {

    /**
     * One-character sentinel kept in the hidden {@link EditText} so the backspace
     * key always has something to delete (and therefore keeps firing) even when the
     * logical field is empty.
     */
    private static final String PLACEHOLDER = " ";

    private EditText inputView;
    private boolean suppressWatcher = false;

    /** Implemented in Rust ({@code Java_com_aimer_AimerActivity_nativeInsertText}). */
    public static native void nativeInsertText(String text);

    /** Implemented in Rust ({@code Java_com_aimer_AimerActivity_nativeBackspace}). */
    public static native void nativeBackspace();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        runOnUiThread(this::setupInputView);
    }

    private void setupInputView() {
        EditText view = new EditText(this);
        // Effectively invisible but still focusable so it can own the IME session.
        view.setAlpha(0f);
        view.setFocusable(true);
        view.setFocusableInTouchMode(true);
        view.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        view.setImeOptions(android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI);

        view.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}

            @Override
            public void onTextChanged(CharSequence s, int start, int before, int count) {}

            @Override
            public void afterTextChanged(Editable s) {
                if (suppressWatcher) {
                    return;
                }

                // While the IME is composing (e.g. Pinyin candidates), the text
                // carries a "composing" span. Wait for the user to commit before
                // forwarding anything so partial composition is never inserted.
                int composeStart = BaseInputConnection.getComposingSpanStart(s);
                int composeEnd = BaseInputConnection.getComposingSpanEnd(s);
                if (composeStart != -1 && composeEnd != -1 && composeEnd > composeStart) {
                    return;
                }

                String text = s.toString();
                if (text.length() > PLACEHOLDER.length()) {
                    // Everything past the sentinel is freshly committed text.
                    String committed = text.substring(PLACEHOLDER.length());
                    nativeInsertText(committed);
                    resetPlaceholder();
                } else if (text.isEmpty()) {
                    // The sentinel itself was deleted -> backspace past the start.
                    nativeBackspace();
                    resetPlaceholder();
                }
            }
        });

        ViewGroup.LayoutParams params = new ViewGroup.LayoutParams(1, 1);
        addContentView(view, params);
        inputView = view;
        resetPlaceholder();
    }

    private void resetPlaceholder() {
        if (inputView == null) {
            return;
        }
        suppressWatcher = true;
        inputView.setText(PLACEHOLDER);
        inputView.setSelection(PLACEHOLDER.length());
        suppressWatcher = false;
    }

    /** Called from Rust via JNI when a text field gains focus. */
    @SuppressWarnings("unused")
    public void showKeyboard() {
        runOnUiThread(() -> {
            if (inputView == null) {
                return;
            }
            resetPlaceholder();
            inputView.requestFocus();
            InputMethodManager imm =
                    (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
            if (imm != null) {
                imm.showSoftInput(inputView, InputMethodManager.SHOW_IMPLICIT);
            }
        });
    }

    /** Called from Rust via JNI when the focused text field is dismissed. */
    @SuppressWarnings("unused")
    public void hideKeyboard() {
        runOnUiThread(() -> {
            InputMethodManager imm =
                    (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
            if (imm != null && inputView != null) {
                imm.hideSoftInputFromWindow(inputView.getWindowToken(), 0);
            }
        });
    }
}
