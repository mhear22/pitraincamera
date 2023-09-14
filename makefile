clone:
	git clone https://github.com/mhear22/pitraincamera.git && cd pitraincamera && make install && make run

install:
	python3 -m pip install -r requirements.txt

run:
	cd src/mysite && python3 manage.py runserver 0.0.0.0:8000
