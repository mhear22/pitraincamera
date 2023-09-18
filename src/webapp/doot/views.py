from django.http import HttpResponse
from rest_framework.views import APIView
from rest_framework.response import Response
from rest_framework.renderers import TemplateHTMLRenderer
import subprocess
from rest_framework.decorators import api_view


class DootModel(APIView):
    renderer_classes = [TemplateHTMLRenderer]
    template_name = 'index.html'


    def get(self, request, format=None):
        return Response()


    def post(self, request, format=None):
        print('Hit')
        return Response({'body': 'done!'})
