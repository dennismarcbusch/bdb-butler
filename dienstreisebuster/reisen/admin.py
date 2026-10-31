from django.contrib import admin

from .models import Anhang, Einstellungen, Reise


class AnhangInline(admin.TabularInline):
    model = Anhang
    extra = 1


@admin.register(Reise)
class ReiseAdmin(admin.ModelAdmin):
    list_display = ("reise_beginn", "zweck", "geschaeftsort", "status", "bezahlt_am")
    list_filter = ("status",)
    inlines = [AnhangInline]


@admin.register(Einstellungen)
class EinstellungenAdmin(admin.ModelAdmin):
    def has_add_permission(self, request):
        return not Einstellungen.objects.exists()

    def has_delete_permission(self, request, obj=None):
        return False
