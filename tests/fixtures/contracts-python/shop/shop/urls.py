from django.urls import include, path

from . import views

urlpatterns = [
    path("accounts/<int:account_id>/", views.account_detail, name="account-detail"),
    path("accounts/", views.account_list),
    path("api/", include("shop.api.urls")),
]
