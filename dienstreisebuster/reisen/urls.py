from django.urls import path

from . import views

app_name = "reisen"

urlpatterns = [
    path("", views.liste, name="liste"),
    path("reise/neu/", views.neu, name="neu"),
    path("reise/<int:pk>/", views.detail, name="detail"),
    path("reise/<int:pk>/bearbeiten/", views.bearbeiten, name="bearbeiten"),
    path("reise/<int:pk>/abrechnung/", views.abrechnung_speichern, name="abrechnung"),
    path("reise/<int:pk>/status/", views.status_speichern, name="status"),
    path("reise/<int:pk>/antrag/erzeugen/", views.antrag_erzeugen, name="antrag_erzeugen"),
    path("reise/<int:pk>/abrechnung/erzeugen/", views.abrechnung_erzeugen, name="abrechnung_erzeugen"),
    path("reise/<int:pk>/versand/<str:art>/", views.versand, name="versand"),
    path("reise/<int:pk>/genehmigung/", views.genehmigung_hochladen, name="genehmigung"),
    path("reise/<int:pk>/anhang/", views.anhang_hinzufuegen, name="anhang_neu"),
    path("anhang/<int:pk>/loeschen/", views.anhang_loeschen, name="anhang_loeschen"),
    path("datei/<int:pk>/<str:art>/", views.datei, name="datei"),
    path("einstellungen/", views.einstellungen, name="einstellungen"),
]
