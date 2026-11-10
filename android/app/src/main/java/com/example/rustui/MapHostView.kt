package com.example.rustui

import android.content.Context
import android.graphics.Color
import android.util.TypedValue
import android.view.Gravity
import android.widget.FrameLayout
import android.widget.TextView
import com.google.android.gms.maps.CameraUpdateFactory
import com.google.android.gms.maps.GoogleMap
import com.google.android.gms.maps.MapView
import com.google.android.gms.maps.model.BitmapDescriptorFactory
import com.google.android.gms.maps.model.LatLng
import com.google.android.gms.maps.model.MapStyleOptions
import com.google.android.gms.maps.model.MarkerOptions
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.ln

/**
 * Native Google Maps host for the `map` primitive. The Rust map leaf only
 * carries data; Google Maps owns panning, zooming, marker layout and the map
 * surface itself.
 */
class MapHostView(context: Context) : FrameLayout(context), NativeMapHost {
    private val mapView: MapView?
    private var googleMap: GoogleMap? = null
    private var pendingProps: JSONObject? = null
    private var started = false
    private var resumed = false

    init {
        setBackgroundColor(Color.parseColor("#101826"))

        if (BuildConfig.MAPS_API_KEY.isBlank()) {
            mapView = null
            if (BuildConfig.DEBUG) {
                addView(
                    TextView(context).apply {
                        text = "Set MAPS_API_KEY to load Google Maps"
                        setTextColor(Color.parseColor("#a3a3a3"))
                        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                        gravity = Gravity.CENTER
                    },
                    LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT),
                )
            }
        } else {
            val nativeMap = MapView(context)
            nativeMap.onCreate(null)
            addView(
                nativeMap,
                LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT),
            )
            mapView = nativeMap
            nativeMap.getMapAsync { map ->
                googleMap = map
                map.uiSettings.isCompassEnabled = false
                map.uiSettings.isMapToolbarEnabled = false
                map.uiSettings.isMyLocationButtonEnabled = false
                pendingProps?.let { applyToMap(map, it) }
            }
        }
    }

    override fun start() {
        if (!started && mapView != null) {
            mapView.onStart()
            started = true
        }
    }

    override fun resume() {
        if (!resumed && mapView != null) {
            mapView.onResume()
            resumed = true
        }
    }

    override fun pause() {
        if (resumed && mapView != null) {
            mapView.onPause()
            resumed = false
        }
    }

    override fun stop() {
        if (started && mapView != null) {
            mapView.onStop()
            started = false
        }
    }

    override fun destroy() {
        pause()
        stop()
        mapView?.onDestroy()
        googleMap = null
    }

    override fun apply(props: JSONObject) {
        pendingProps = props
        googleMap?.let { applyToMap(it, props) }
    }

    private fun applyToMap(map: GoogleMap, props: JSONObject) {
        val dark = props.optBoolean("mapDark", true)
        map.setMapStyle(if (dark) MapStyleOptions(DARK_STYLE) else null)

        val latitude = props.optDouble("mapLatitude", Double.NaN)
        val longitude = props.optDouble("mapLongitude", Double.NaN)
        val latitudeDelta = props.optDouble("mapLatitudeDelta", 0.025)
        if (!latitude.isNaN() && !longitude.isNaN()) {
            val zoom = (ln(360.0 / latitudeDelta.coerceAtLeast(0.0001)) / ln(2.0))
                .toFloat()
                .coerceIn(2f, 20f)
            map.moveCamera(
                CameraUpdateFactory.newLatLngZoom(LatLng(latitude, longitude), zoom),
            )
        }

        map.clear()
        val rawMarkers = props.optString("mapMarkers", "")
        if (rawMarkers.isNotBlank()) {
            val markers = JSONArray(rawMarkers)
            for (index in 0 until markers.length()) {
                val marker = markers.optJSONObject(index) ?: continue
                map.addMarker(
                    MarkerOptions()
                        .position(
                            LatLng(
                                marker.optDouble("lat"),
                                marker.optDouble("lng"),
                            ),
                        )
                        .title(marker.optString("name"))
                        .icon(
                            BitmapDescriptorFactory.defaultMarker(
                                BitmapDescriptorFactory.HUE_ORANGE,
                            ),
                        ),
                )
            }
        }
    }

    private companion object {
        const val DARK_STYLE = """
            [
              {"elementType":"geometry","stylers":[{"color":"#212121"}]},
              {"elementType":"labels.icon","stylers":[{"visibility":"off"}]},
              {"elementType":"labels.text.fill","stylers":[{"color":"#ffffff"}]},
              {"elementType":"labels.text.stroke","stylers":[{"color":"#000000"}]},
              {"featureType":"water","stylers":[{"color":"#000000"}]}
            ]
        """
    }
}
