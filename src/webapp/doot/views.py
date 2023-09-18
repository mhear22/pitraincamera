from django.http import HttpResponse
from rest_framework.views import APIView
from rest_framework.response import Response
from rest_framework.renderers import TemplateHTMLRenderer
import subprocess


class DootModel(APIView):
    renderer_classes = [TemplateHTMLRenderer]
    template_name = 'index.html'


    def get(self, request, format=None):
        return Response()


    def post(self, request, format=None):
        print('Hit')
        return Response({'body': 'done!'})
    
    def photo(self, request, format=None):
        try:
            cmd = "raspistill -vf -o /home/pi/pitraincamera/images/pic.jpeg"
            subprocess.call(cmd, shell=True)
        except:
            print("Failed to trigger camera")
        return Response({"body": 'photo taken'})
    
    