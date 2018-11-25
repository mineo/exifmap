var map = L.map('map-canvas').setView([50.683889,10.919444], 4);
var osmLayer = L.tileLayer('//{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
    attribution: 'Map data &copy; <a href="http://openstreetmap.org">OpenStreetMap</a> contributors.',
    maxZoom: 19
});
map.addLayer(osmLayer);

var googleLayer = new L.gridLayer.googleMutant({type: "HYBRID"});

var tileLayers = {
    'OpenStreetMap': osmLayer,
    'Google': googleLayer
};

L.control.layers(tileLayers).addTo(map);

function featureToPopup(feature){
    mbid = feature.key;
    name = feature.properties.name;
    coordinates = feature.properties.coordinates;
    var marker = L.marker(coordinates, {'title': name});
    var info = document.createElement("div");

    var name = document.createElement("h3");
    name.textContent = feature.properties.name;
    info.appendChild(name);

    if ('thumbnail_filename' in feature.properties){
        var imagelink = document.createElement("a");
        imagelink.href = feature.properties.commons_link;
        imagelink.target = "_blank";

        var image = document.createElement("img");
        image.src = "/output/" + feature.properties.thumbnail_filename;

        imagelink.appendChild(image);

        info.appendChild(imagelink);
        info.appendChild(document.createElement("br"));
    }
}

var hash = new L.Hash(map);
$.getJSON("/output/data.json").done(function(data){
    var markerCluster = new L.MarkerClusterGroup();
    var geoJsonLayer = L.geoJson(data, {
        onEachFeature: function(feature, layer) {
            layer.bindPopup(featureToPopup(feature));
        }
    });
    markerCluster.addLayer(geoJsonLayer);
    map.addLayer(markerCluster);
});
