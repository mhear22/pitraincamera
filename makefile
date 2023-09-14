clone:
	git clone https://github.com/mhear22/pitraincamera.git

install-deps:
	python3 -m pip install -r requirements.txt

run:
	cd src/mysite && python3 manage.py runserver

install: clone install-deps run
	echo 'installing'