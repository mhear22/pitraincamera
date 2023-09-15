from django.urls import path

from . import views

urlpatterns = [
    path("", views.DootModel.as_view()),
]