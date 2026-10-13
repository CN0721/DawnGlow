package top.izuna.dawnglow;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.Context;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.util.Log;
import android.widget.RemoteViews;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/**
 * 朝晚霞桌面小组件。
 *
 * <p>RemoteViews 不能用自定义 View 自绘，但可以把「自己用 Canvas 画好的 Bitmap」
 * 通过 {@code setImageViewBitmap} 塞进 ImageView——进度条即用此法绘制。
 */
public class GlowWidgetProvider extends AppWidgetProvider {

    private static final String TAG = "DawnGlowWidget";
    private static final double DEFAULT_LAT = 30.2741;
    private static final double DEFAULT_LON = 120.1551;
    private static final int TRACK_COLOR = 0xFF2A2F45;

    @Override
    public void onUpdate(Context context, AppWidgetManager manager, int[] ids) {
        for (int id : ids) {
            render(context, manager, id);
        }
    }

    private void render(Context context, AppWidgetManager manager, int id) {
        try {
            String settings = readSettings(context);
            String name = str(settings, "name");
            if (name == null || name.isEmpty()) {
                name = "当前地点";
            }
            Boolean compact = bool(settings, "compact_name");
            if (compact == null || compact) {
                int dot = name.indexOf(" · ");
                if (dot > 0) {
                    name = name.substring(0, dot);
                }
            }
            Double lat = num(settings, "lat");
            Double lon = num(settings, "lon");
            final double flat = lat != null ? lat : DEFAULT_LAT;
            final double flon = lon != null ? lon : DEFAULT_LON;
            final String place = name;

            RemoteViews loading = baseViews(context);
            loading.setTextViewText(R.id.widget_title, place);
            loading.setTextViewText(R.id.widget_date, "更新中…");
            manager.updateAppWidget(id, loading);

            final Context appCtx = context.getApplicationContext();
            final int widgetId = id;
            new Thread(() -> {
                String body;
                try {
                    body = WidgetBridge.forecast(flat, flon, appCtx.getFilesDir().getAbsolutePath());
                } catch (Throwable t) {
                    Log.e(TAG, "native forecast failed", t);
                    body = "数据获取失败\n" + t.getClass().getSimpleName();
                }
                try {
                    RemoteViews views = baseViews(appCtx);
                    views.setTextViewText(R.id.widget_title, place);
                    fillViews(views, body);
                    AppWidgetManager.getInstance(appCtx).updateAppWidget(widgetId, views);
                } catch (Throwable t) {
                    Log.e(TAG, "widget update failed", t);
                }
            }).start();
        } catch (Throwable t) {
            Log.e(TAG, "render failed", t);
        }
    }

    /** Rust 侧返回 9 行字段；行数足够则逐项填充，否则整体回退到一行提示。 */
    private static void fillViews(RemoteViews rv, String body) {
        String[] p = body.split("\n", -1);
        if (p.length >= 9) {
            rv.setTextViewText(R.id.widget_date, p[0]);
            rv.setTextViewText(R.id.widget_sunrise_time, p[1]);
            applyScore(rv, R.id.widget_sunrise_score, R.id.widget_sunrise_bar, parseInt(p[2]));
            rv.setTextViewText(R.id.widget_sunset_time, p[3]);
            applyScore(rv, R.id.widget_sunset_score, R.id.widget_sunset_bar, parseInt(p[4]));
            rv.setTextViewText(R.id.widget_clouds, p[5]);
            rv.setTextViewText(R.id.widget_env1, p[6]);
            rv.setTextViewText(R.id.widget_env2, p[7]);
            rv.setTextViewText(R.id.widget_type, p[8]);
        } else {
            rv.setTextViewText(R.id.widget_date, "");
            rv.setTextViewText(R.id.widget_sunrise_time, "");
            rv.setTextViewText(R.id.widget_sunrise_score, "");
            rv.setImageViewBitmap(R.id.widget_sunrise_bar, barBitmap(0, TRACK_COLOR));
            rv.setTextViewText(R.id.widget_sunset_time, "");
            rv.setTextViewText(R.id.widget_sunset_score, "");
            rv.setImageViewBitmap(R.id.widget_sunset_bar, barBitmap(0, TRACK_COLOR));
            rv.setTextViewText(R.id.widget_clouds, "");
            rv.setTextViewText(R.id.widget_env1, "");
            rv.setTextViewText(R.id.widget_env2, "");
            rv.setTextViewText(R.id.widget_type, body.replace("\n", " "));
        }
    }

    /** 百分比文本按分数着色，进度条用自绘 Bitmap。 */
    private static void applyScore(RemoteViews rv, int pctId, int barId, int value) {
        int v = Math.max(0, Math.min(100, value));
        int color = 0xFF8E93A8;
        if (v >= 75) {
            color = 0xFFFF7A59;
        } else if (v >= 50) {
            color = 0xFFFFB454;
        } else if (v >= 30) {
            color = 0xFFFFD479;
        }
        rv.setTextViewText(pctId, v + "%");
        rv.setTextColor(pctId, color);
        rv.setImageViewBitmap(barId, barBitmap(v, color));
    }

    /** 用 Canvas 画一根圆角进度条（轨道 + 按比例填充），供 ImageView 以 fitXY 拉伸。 */
    private static Bitmap barBitmap(int percent, int color) {
        final int w = 320;
        final int h = 24;
        Bitmap bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888);
        Canvas c = new Canvas(bmp);
        Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
        float r = h / 2f;
        p.setColor(TRACK_COLOR);
        c.drawRoundRect(new RectF(0, 0, w, h), r, r, p);
        if (percent > 0) {
            float fw = Math.max(w * percent / 100f, h * 0.35f);
            float fr = Math.min(r, fw / 2f);
            p.setColor(color);
            c.drawRoundRect(new RectF(0, 0, fw, h), fr, fr, p);
        }
        return bmp;
    }

    private static int parseInt(String s) {
        try {
            return Integer.parseInt(s.trim().replaceAll("[^0-9]", ""));
        } catch (Exception e) {
            return 0;
        }
    }

    private RemoteViews baseViews(Context context) {
        RemoteViews rv = new RemoteViews(context.getPackageName(), R.layout.widget_glow);
        Intent launch = context.getPackageManager()
                .getLaunchIntentForPackage(context.getPackageName());
        if (launch != null) {
            launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            PendingIntent pi = PendingIntent.getActivity(
                    context, 0, launch,
                    PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            rv.setOnClickPendingIntent(R.id.widget_root, pi);
        }
        return rv;
    }

    private static String readSettings(Context context) {
        File f = new File(context.getFilesDir(), "settings.json");
        if (!f.exists()) {
            return "";
        }
        try (InputStream in = new FileInputStream(f)) {
            byte[] buf = new byte[(int) f.length()];
            int n = in.read(buf);
            return new String(buf, 0, Math.max(n, 0), StandardCharsets.UTF_8);
        } catch (Exception e) {
            return "";
        }
    }

    private static Double num(String json, String key) {
        int i = json.indexOf('"' + key + '"');
        if (i < 0) return null;
        int c = json.indexOf(':', i + key.length() + 2);
        if (c < 0) return null;
        int s = c + 1;
        while (s < json.length() && Character.isWhitespace(json.charAt(s))) s++;
        int e = s;
        while (e < json.length() && "-+.0123456789eE".indexOf(json.charAt(e)) >= 0) e++;
        try {
            return Double.parseDouble(json.substring(s, e));
        } catch (Exception ex) {
            return null;
        }
    }

    private static String str(String json, String key) {
        int i = json.indexOf('"' + key + '"');
        if (i < 0) return null;
        int colon = json.indexOf(':', i + key.length() + 2);
        if (colon < 0) return null;
        int c = json.indexOf('"', colon + 1);
        if (c < 0) return null;
        int e = json.indexOf('"', c + 1);
        if (e < 0) return null;
        return json.substring(c + 1, e);
    }

    private static Boolean bool(String json, String key) {
        int i = json.indexOf('"' + key + '"');
        if (i < 0) return null;
        int c = json.indexOf(':', i + key.length() + 2);
        if (c < 0) return null;
        String rest = json.substring(c + 1).trim();
        if (rest.startsWith("true")) return Boolean.TRUE;
        if (rest.startsWith("false")) return Boolean.FALSE;
        return null;
    }
}
