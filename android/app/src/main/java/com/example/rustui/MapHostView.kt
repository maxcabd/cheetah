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
import com.google.android.gms.maps.model.Marker
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
    override var onMarker: ((String) -> Unit)? = null
    private val mapView: MapView?
    private var googleMap: GoogleMap? = null
    private var pendingProps: JSONObject? = null
    private var lastMarkers: String? = null
    private var lastRegion: String? = null
    private var lastDark: Boolean? = null
    private var lastShowsUser: Boolean? = null
    private var markerStructure: String? = null
    private val markersById = HashMap<String, Marker>()
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
                map.setOnMarkerClickListener { marker ->
                    val id = marker.tag as? String
                    if (!id.isNullOrEmpty()) {
                        onMarker?.invoke(id)
                    }
                    true
                }
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
        if (dark != lastDark) {
            map.setMapStyle(if (dark) MapStyleOptions(DARK_STYLE) else null)
            lastDark = dark
        }

        val latitude = props.optDouble("mapLatitude", Double.NaN)
        val longitude = props.optDouble("mapLongitude", Double.NaN)
        val latitudeDelta = props.optDouble("mapLatitudeDelta", 0.025)
        val regionKey = "$latitude,$longitude,$latitudeDelta"
        if (!latitude.isNaN() && !longitude.isNaN() && regionKey != lastRegion) {
            val zoom = (ln(360.0 / latitudeDelta.coerceAtLeast(0.0001)) / ln(2.0))
                .toFloat()
                .coerceIn(2f, 20f)
            val update = CameraUpdateFactory.newLatLngZoom(LatLng(latitude, longitude), zoom)
            if (lastRegion == null) {
                map.moveCamera(update)
            } else {
                map.animateCamera(update, 420, null)
            }
            lastRegion = regionKey
        }

        val showsUser = props.optBoolean("mapShowsUser", false)
        if (showsUser != lastShowsUser) {
            try {
                map.isMyLocationEnabled = showsUser
            } catch (_: SecurityException) {
                // The map still works without the user-location layer.
            }
            lastShowsUser = showsUser
        }

        val rawMarkers = props.optString("mapMarkers", "")
        if (rawMarkers == lastMarkers) {
            return
        }
        lastMarkers = rawMarkers
        val markers = parseMarkers(rawMarkers)
        val structure = markerStructure(markers)
        if (structure == markerStructure && markersById.size == markers.size) {
            updateMarkerImages(markers)
            return
        }
        markerStructure = structure
        map.clear()
        markersById.clear()
        for (marker in markers) {
            val added = addMarker(map, marker) ?: continue
            markersById[marker.optString("id")] = added
        }
    }

    private fun parseMarkers(raw: String): List<JSONObject> {
        if (raw.isBlank()) return emptyList()
        val array = JSONArray(raw)
        return buildList(array.length()) {
            for (index in 0 until array.length()) {
                array.optJSONObject(index)?.let(::add)
            }
        }
    }

    private fun markerStructure(markers: List<JSONObject>): String =
        markers.joinToString("|") {
            val id = it.optString("id")
            "${id}@${it.optDouble("lat")},${it.optDouble("lng")}"
        }

    private fun addMarker(map: GoogleMap, marker: JSONObject): Marker? {
        val added = map.addMarker(
            MarkerOptions()
                .position(
                    LatLng(
                        marker.optDouble("lat"),
                        marker.optDouble("lng"),
                    ),
                )
                .title(marker.optString("name"))
                .anchor(0.5f, 0.5f)
                .icon(markerIcon(marker)),
        )
        added?.tag = marker.optString("id")
        return added
    }

    private fun updateMarkerImages(markers: List<JSONObject>) {
        for (marker in markers) {
            val existing = markersById[marker.optString("id")] ?: continue
            existing.title = marker.optString("name")
            existing.setIcon(markerIcon(marker))
        }
    }

    private fun markerIcon(marker: JSONObject) = BitmapDescriptorFactory.fromBitmap(
        MarkerBitmapFactory.create(
            context,
            marker.optString("kind", "static"),
            marker.optString("icon", "utensils"),
            marker.optString("iconColor", "#D32F2F"),
            marker.optBoolean("selected", false),
        ),
    )

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
