from django.http import HttpResponse
from rest_framework.views import APIView
from rest_framework.response import Response
from rest_framework.renderers import TemplateHTMLRenderer
import subprocess
from rest_framework.decorators import api_view
import os


# Create your views here.
class ImageModel(APIView):
    renderer_classes = [TemplateHTMLRenderer]
    template_name = 'image.html'


    def get(self, request, format=None):
        here = os.getcwd()
        images = os.listdir(os.path.join(here,'/src/webapp/photos/images'))
        return Response({'images': images})
    
    def get_image(self, request, format=None):
    
        return Response({"body": 'photo taken'})


    def put(self, request, format=None):
        try:
            cmd = "raspistill -vf -o /home/mhear22/pitraincamera/src/webapp/photos/images/pic.jpeg"
            subprocess.call(cmd, shell=True)
        except:
            print("Failed to trigger camera")
        return Response({"body": 'photo taken'})

    def post(self, request, format=None):
        return Response({"body": 'photo taken'})
    