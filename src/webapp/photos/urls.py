from django.urls import path

from . import views

urlpatterns = [
    path("", views.ImageModel.as_view()),
]