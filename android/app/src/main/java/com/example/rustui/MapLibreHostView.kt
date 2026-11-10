package com.example.rustui

import android.content.Context
import android.graphics.Color
import android.util.TypedValue
import android.view.Gravity
import android.widget.FrameLayout
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point
import kotlin.math.ln

/**
 * Keyless native map fallback. MapLibre owns the map surface; Rust still owns
 * the region and marker data passed through the `map` primitive.
 */
class MapLibreHostView(context: Context) : FrameLayout(context), NativeMapHost {
    private val mapView: MapView
    private var map: MapLibreMap? = null
    private var style: Style? = null
    private var pendingProps: JSONObject? = null
    private var started = false
    private var resumed = false

    init {
        setBackgroundColor(Color.parseColor("#101826"))
        MapLibre.getInstance(context)

        mapView = MapView(context)
        mapView.onCreate(null)
        addView(mapView, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))

        addView(
            TextView(context).apply {
                text = "© OpenFreeMap © OpenStreetMap contributors"
                setTextColor(Color.parseColor("#d4d4d4"))
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 9f)
                setPadding(8, 2, 8, 2)
                setBackgroundColor(Color.parseColor("#66000000"))
            },
            LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.START),
        )

        mapView.getMapAsync { nativeMap ->
            map = nativeMap
            nativeMap.uiSettings.isCompassEnabled = false
            nativeMap.uiSettings.isRotateGesturesEnabled = false
            nativeMap.setStyle(Style.Builder().fromUri(STYLE_URL)) { loadedStyle ->
                style = loadedStyle
                pendingProps?.let { applyToMap(nativeMap, loadedStyle, it) }
            }
        }
    }

    override fun start() {
        if (!started) {
            mapView.onStart()
            started = true
        }
    }

    override fun resume() {
        if (!resumed) {
            mapView.onResume()
            resumed = true
        }
    }

    override fun pause() {
        if (resumed) {
            mapView.onPause()
            resumed = false
        }
    }

    override fun stop() {
        if (started) {
            mapView.onStop()
            started = false
        }
    }

    override fun destroy() {
        pause()
        stop()
        mapView.onDestroy()
        map = null
        style = null
    }

    override fun apply(props: JSONObject) {
        pendingProps = props
        val nativeMap = map ?: return
        val loadedStyle = style ?: return
        applyToMap(nativeMap, loadedStyle, props)
    }

    private fun applyToMap(nativeMap: MapLibreMap, loadedStyle: Style, props: JSONObject) {
        val latitude = props.optDouble("mapLatitude", Double.NaN)
        val longitude = props.optDouble("mapLongitude", Double.NaN)
        val latitudeDelta = props.optDouble("mapLatitudeDelta", 0.025)
        if (!latitude.isNaN() && !longitude.isNaN()) {
            val zoom = (ln(360.0 / latitudeDelta.coerceAtLeast(0.0001)) / ln(2.0))
                .coerceIn(2.0, 20.0)
            nativeMap.cameraPosition = CameraPosition.Builder()
                .target(LatLng(latitude, longitude))
                .zoom(zoom)
                .build()
        }

        loadedStyle.getLayer(PLACES_LAYER)?.let { loadedStyle.removeLayer(it) }
        loadedStyle.getSource(PLACES_SOURCE)?.let { loadedStyle.removeSource(it) }

        val rawMarkers = props.optString("mapMarkers", "")
        if (rawMarkers.isBlank()) {
            return
        }

        val markers = JSONArray(rawMarkers)
        val features = mutableListOf<Feature>()
        for (index in 0 until markers.length()) {
            val marker = markers.optJSONObject(index) ?: continue
            features.add(
                Feature.fromGeometry(
                    Point.fromLngLat(marker.optDouble("lng"), marker.optDouble("lat")),
                ),
            )
        }
        if (features.isEmpty()) {
            return
        }

        loadedStyle.addSource(
            GeoJsonSource(PLACES_SOURCE, FeatureCollection.fromFeatures(features)),
        )
        loadedStyle.addLayer(
            CircleLayer(PLACES_LAYER, PLACES_SOURCE).withProperties(
                PropertyFactory.circleColor("#e5522f"),
                PropertyFactory.circleRadius(7f),
                PropertyFactory.circleStrokeColor("#ffffff"),
                PropertyFactory.circleStrokeWidth(2f),
            ),
        )
    }

    private companion object {
        const val PLACES_SOURCE = "cheetah-places"
        const val PLACES_LAYER = "cheetah-places-layer"
        const val STYLE_URL = "https://tiles.openfreemap.org/styles/dark"
    }
}
