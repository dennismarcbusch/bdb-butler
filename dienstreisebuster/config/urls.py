from django.contrib import admin
from django.urls import include, path

urlpatterns = [
    path("admin/", admin.site.urls),
    path("konto/", include("django.contrib.auth.urls")),
    path("", include("reisen.urls")),
]
