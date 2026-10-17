package com.ghostmusic.app;

import android.app.Activity;
import android.app.DownloadManager;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Color;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.provider.MediaStore;
import android.text.InputType;
import android.util.Base64;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.Menu;
import android.view.MenuItem;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.webkit.CookieManager;
import android.webkit.DownloadListener;
import android.webkit.JavascriptInterface;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;
import android.widget.Toast;

import java.io.File;
import java.io.FileOutputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.Scanner;

/**
 * Ghost Music for Android.
 *
 * The library, extraction pipeline and database live on the Ghost Music server
 * (the desktop app, or any hosted deployment). This shell connects to that
 * address, keeps the responsive web app in a WebView, and adds two things a
 * plain browser tab cannot do: it remembers the server across launches and it
 * saves downloaded MP3s into the device's Music folder through a JS bridge.
 */
public class MainActivity extends Activity {
    private static final String PREFS = "ghost_music";
    private static final String KEY_SERVER = "server_url";
    private static final int COLOR_BG = 0xFF0B0B0E;
    private static final int COLOR_SURFACE = 0xFF15151C;
    private static final int COLOR_TEXT = 0xFFF4EEF1;
    private static final int COLOR_MUTED = 0xFFA79AA3;
    private static final int COLOR_ACCENT = 0xFFFF5C74;

    private FrameLayout root;
    private WebView webView;
    private String serverUrl;
    private GhostBridge bridge;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        root = new FrameLayout(this);
        root.setBackgroundColor(COLOR_BG);
        setContentView(root);
        bridge = new GhostBridge();
        serverUrl = prefs().getString(KEY_SERVER, null);
        if (serverUrl == null) {
            showSetup(getString(R.string.connect_body), "", false);
        } else {
            loadApp(serverUrl);
        }
    }

    private SharedPreferences prefs() {
        return getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    private int dp(float value) {
        return Math.round(TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, getResources().getDisplayMetrics()));
    }

    /* ------------------------------------------------------------------ setup */
    private void showSetup(String message, String existing, boolean showError) {
        releaseWebView();
        root.removeAllViews();

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        column.setGravity(Gravity.CENTER_HORIZONTAL);
        column.setPadding(dp(28), dp(48), dp(28), dp(28));

        ImageView icon = new ImageView(this);
        icon.setImageResource(R.mipmap.ic_launcher);
        LinearLayout.LayoutParams iconParams = new LinearLayout.LayoutParams(dp(84), dp(84));
        iconParams.bottomMargin = dp(22);
        column.addView(icon, iconParams);

        TextView title = new TextView(this);
        title.setText(R.string.connect_title);
        title.setTextColor(COLOR_TEXT);
        title.setTextSize(26);
        title.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        column.addView(title);

        TextView body = new TextView(this);
        body.setText(message);
        body.setTextColor(COLOR_MUTED);
        body.setTextSize(14);
        body.setLineSpacing(dp(3), 1f);
        LinearLayout.LayoutParams bodyParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        bodyParams.topMargin = dp(12);
        column.addView(body, bodyParams);

        EditText input = new EditText(this);
        input.setHint(R.string.connect_hint);
        input.setText(existing);
        input.setSingleLine(true);
        input.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
        input.setImeOptions(EditorInfo.IME_ACTION_GO);
        input.setTextColor(COLOR_TEXT);
        input.setHintTextColor(0xFF6E636D);
        input.setTextSize(16);
        input.setBackgroundColor(COLOR_SURFACE);
        input.setPadding(dp(16), dp(16), dp(16), dp(16));
        LinearLayout.LayoutParams inputParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        inputParams.topMargin = dp(22);
        column.addView(input, inputParams);

        TextView error = new TextView(this);
        error.setTextColor(0xFFFF7A8B);
        error.setTextSize(13);
        error.setVisibility(View.GONE);
        if (showError) {
            error.setText(message);
            error.setVisibility(View.VISIBLE);
            body.setVisibility(View.GONE);
        }
        LinearLayout.LayoutParams errorParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        errorParams.topMargin = dp(12);
        column.addView(error, errorParams);

        Button connect = new Button(this);
        connect.setText(R.string.connect_action);
        connect.setAllCaps(false);
        connect.setTextSize(16);
        connect.setTextColor(Color.WHITE);
        connect.setBackgroundTintList(android.content.res.ColorStateList.valueOf(COLOR_ACCENT));
        LinearLayout.LayoutParams buttonParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(52));
        buttonParams.topMargin = dp(18);
        column.addView(connect, buttonParams);

        ProgressBar progress = new ProgressBar(this);
        progress.setIndeterminate(true);
        progress.setVisibility(View.GONE);
        LinearLayout.LayoutParams progressParams = new LinearLayout.LayoutParams(dp(32), dp(32));
        progressParams.topMargin = dp(18);
        column.addView(progress, progressParams);

        TextView status = new TextView(this);
        status.setText(R.string.connect_checking);
        status.setTextColor(COLOR_MUTED);
        status.setTextSize(13);
        status.setGravity(Gravity.CENTER);
        status.setVisibility(View.GONE);
        LinearLayout.LayoutParams statusParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        statusParams.topMargin = dp(10);
        column.addView(status, statusParams);

        Runnable submit = () -> {
            String normalized = normalize(input.getText().toString());
            if (normalized == null) {
                error.setText("Enter the address shown in Ghost Music Desktop, for example 192.168.1.20:43110.");
                error.setVisibility(View.VISIBLE);
                return;
            }
            error.setVisibility(View.GONE);
            progress.setVisibility(View.VISIBLE);
            status.setVisibility(View.VISIBLE);
            connect.setEnabled(false);
            new Thread(() -> {
                boolean reachable = healthCheck(normalized);
                runOnUiThread(() -> {
                    progress.setVisibility(View.GONE);
                    status.setVisibility(View.GONE);
                    connect.setEnabled(true);
                    if (reachable) {
                        prefs().edit().putString(KEY_SERVER, normalized).apply();
                        serverUrl = normalized;
                        loadApp(normalized);
                    } else {
                        error.setText(R.string.connect_error);
                        error.setVisibility(View.VISIBLE);
                    }
                });
            }).start();
        };

        connect.setOnClickListener(v -> submit.run());
        input.setOnEditorActionListener((v, actionId, event) -> {
            submit.run();
            return true;
        });

        root.addView(column, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    }

    /** Accepts "192.168.1.20:43110", "http://host:port/", or a full URL. */
    private String normalize(String raw) {
        if (raw == null) return null;
        String value = raw.trim();
        if (value.isEmpty()) return null;
        if (!value.contains("://")) value = "http://" + value;
        value = value.replaceAll("/+$", "");
        try {
            Uri uri = Uri.parse(value);
            String host = uri.getHost();
            if (host == null || host.isEmpty()) return null;
            if (!"http".equals(uri.getScheme()) && !"https".equals(uri.getScheme())) return null;
            return value;
        } catch (Exception error) {
            return null;
        }
    }

    private boolean healthCheck(String base) {
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(base + "/api/health").openConnection();
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(7000);
            connection.setRequestMethod("GET");
            if (connection.getResponseCode() != 200) return false;
            Scanner scanner = new Scanner(connection.getInputStream()).useDelimiter("\\A");
            String body = scanner.hasNext() ? scanner.next() : "";
            scanner.close();
            return body.contains("\"ok\":true");
        } catch (Exception error) {
            return false;
        } finally {
            if (connection != null) connection.disconnect();
        }
    }

    /* ------------------------------------------------------------- web view */
    private void loadApp(final String url) {
        root.removeAllViews();
        if (webView == null) {
            webView = new WebView(this);
            webView.setBackgroundColor(COLOR_BG);
            WebSettings settings = webView.getSettings();
            settings.setJavaScriptEnabled(true);
            settings.setDomStorageEnabled(true);
            settings.setDatabaseEnabled(true);
            settings.setMediaPlaybackRequiresUserGesture(false);
            settings.setLoadWithOverviewMode(true);
            settings.setUseWideViewPort(true);
            settings.setSupportZoom(false);
            settings.setBuiltInZoomControls(false);
            settings.setDisplayZoomControls(false);
            settings.setTextZoom(100);
            settings.setAllowFileAccess(false);
            settings.setAllowContentAccess(true);
            settings.setUserAgentString(settings.getUserAgentString() + " GhostMusicAndroid/1.0");
            CookieManager.getInstance().setAcceptCookie(true);
            webView.addJavascriptInterface(bridge, "GhostMusic");
            webView.setWebChromeClient(new WebChromeClient());
            webView.setWebViewClient(new WebViewClient() {
                @Override
                public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                    Uri target = request.getUrl();
                    String host = target.getHost();
                    Uri origin = Uri.parse(url);
                    if (host != null && host.equals(origin.getHost())) return false;
                    try {
                        startActivity(new Intent(Intent.ACTION_VIEW, target));
                    } catch (Exception ignored) {
                    }
                    return true;
                }

                @Override
                public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                    if (request.isForMainFrame()) {
                        showSetup(getString(R.string.connect_error), url, true);
                    }
                }
            });
            webView.setDownloadListener(new DownloadListener() {
                @Override
                public void onDownloadStart(String link, String userAgent, String contentDisposition, String mimeType, long length) {
                    if (link.startsWith("blob:") || link.startsWith("data:")) {
                        Toast.makeText(MainActivity.this, "Use the download button inside the app to save this track.", Toast.LENGTH_LONG).show();
                        return;
                    }
                    try {
                        DownloadManager.Request request = new DownloadManager.Request(Uri.parse(link));
                        request.setMimeType(mimeType);
                        request.addRequestHeader("User-Agent", userAgent);
                        request.setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED);
                        request.setDestinationInExternalPublicDir(Environment.DIRECTORY_MUSIC, "Ghost Music/" + Uri.parse(link).getLastPathSegment());
                        DownloadManager manager = (DownloadManager) getSystemService(Context.DOWNLOAD_SERVICE);
                        manager.enqueue(request);
                        Toast.makeText(MainActivity.this, "Saving to Music/Ghost Music", Toast.LENGTH_SHORT).show();
                    } catch (Exception error) {
                        Toast.makeText(MainActivity.this, "This file could not be saved.", Toast.LENGTH_LONG).show();
                    }
                }
            });
        }
        root.addView(webView, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        webView.loadUrl(url);
    }

    private void releaseWebView() {
        if (webView != null) {
            root.removeView(webView);
            webView.stopLoading();
            webView.destroy();
            webView = null;
        }
    }

    /* ------------------------------------------------------------------ menu */
    @Override
    public boolean onCreateOptionsMenu(Menu menu) {
        menu.add(0, 1, 0, R.string.settings);
        menu.add(0, 2, 1, R.string.reload);
        return true;
    }

    @Override
    public boolean onOptionsItemSelected(MenuItem item) {
        if (item.getItemId() == 1) {
            prefs().edit().remove(KEY_SERVER).apply();
            serverUrl = null;
            showSetup(getString(R.string.connect_body), "", false);
            return true;
        }
        if (item.getItemId() == 2) {
            if (webView != null) webView.reload();
            else if (serverUrl != null) loadApp(serverUrl);
            return true;
        }
        return super.onOptionsItemSelected(item);
    }

    @Override
    public void onBackPressed() {
        if (webView != null && webView.canGoBack()) {
            webView.goBack();
            return;
        }
        super.onBackPressed();
    }

    @Override
    protected void onDestroy() {
        releaseWebView();
        super.onDestroy();
    }

    /* --------------------------------------------------------------- bridge */
    /**
     * Receives downloaded MP3 bytes from the web app in chunks and writes them
     * into Music/Ghost Music, which the WebView cannot do on its own because
     * the file is produced as an in-memory blob.
     */
    private class GhostBridge {
        private OutputStream stream;
        private Uri pendingUri;
        private File pendingFile;
        private String currentName;

        @JavascriptInterface
        public boolean saveFile(String name, String base64Chunk, boolean append) {
            try {
                byte[] bytes = Base64.decode(base64Chunk, Base64.DEFAULT);
                if (!append || stream == null) {
                    closeQuietly();
                    if (!open(name)) return false;
                }
                stream.write(bytes);
                return true;
            } catch (Exception error) {
                closeQuietly();
                return false;
            }
        }

        @JavascriptInterface
        public boolean finishFile() {
            try {
                if (stream == null) return false;
                stream.flush();
                closeQuietly();
                final String label = currentName == null ? "Track" : currentName;
                runOnUiThread(() -> Toast.makeText(MainActivity.this, "Saved " + label + " to Music/Ghost Music", Toast.LENGTH_LONG).show());
                return true;
            } catch (Exception error) {
                closeQuietly();
                return false;
            }
        }

        private boolean open(String name) {
            String safe = (name == null ? "Ghost Music track" : name).replaceAll("[\\\\/:*?\"<>|]", "").trim();
            if (!safe.toLowerCase().endsWith(".mp3")) safe = safe + ".mp3";
            currentName = safe;
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    ContentValues values = new ContentValues();
                    values.put(MediaStore.MediaColumns.DISPLAY_NAME, safe);
                    values.put(MediaStore.MediaColumns.MIME_TYPE, "audio/mpeg");
                    values.put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_MUSIC + "/Ghost Music");
                    values.put(MediaStore.MediaColumns.IS_PENDING, 1);
                    pendingUri = getContentResolver().insert(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, values);
                    if (pendingUri == null) return false;
                    stream = getContentResolver().openOutputStream(pendingUri);
                    return stream != null;
                }
                File directory = new File(getExternalFilesDir(Environment.DIRECTORY_MUSIC), "Ghost Music");
                if (!directory.exists() && !directory.mkdirs()) return false;
                pendingFile = new File(directory, safe);
                stream = new FileOutputStream(pendingFile);
                return true;
            } catch (Exception error) {
                return false;
            }
        }

        private void closeQuietly() {
            try {
                if (stream != null) stream.close();
            } catch (Exception ignored) {
            }
            stream = null;
            if (pendingUri != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                ContentValues values = new ContentValues();
                values.put(MediaStore.MediaColumns.IS_PENDING, 0);
                try {
                    getContentResolver().update(pendingUri, values, null, null);
                } catch (Exception ignored) {
                }
            }
            pendingUri = null;
            pendingFile = null;
        }
    }
}
