.PHONY: all clean windows linux macos release release-windows release-linux release-macos

# Nom de ton programme exécutable
EXECUTABLE_NAME := live_chat

all: windows linux macos

windows:
	@echo "Compilation pour Windows..."
	GOOS=windows GOARCH=amd64 go build -o bin/$(EXECUTABLE_NAME)_windows.exe .
	@echo "Compilation Windows terminée."

linux:
	@echo "Compilation pour Linux..."
	GOOS=linux GOARCH=amd64 go build -o bin/$(EXECUTABLE_NAME)_linux .
	@echo "Compilation Linux terminée."

macos:
	@echo "Compilation pour macOS..."
	GOOS=darwin GOARCH=arm64 go build -o bin/$(EXECUTABLE_NAME)_macos .
	@echo "Compilation macOS terminée."

release-windows:
	@echo "Compilation release pour Windows..."
	GOOS=windows GOARCH=amd64 go build -ldflags "-s -w" -o bin/$(EXECUTABLE_NAME)_windows.exe .
	@echo "Release Windows créée."

release-linux:
	@echo "Compilation release pour Linux..."
	GOOS=linux GOARCH=amd64 go build -ldflags "-s -w" -o bin/$(EXECUTABLE_NAME)_linux .
	@echo "Release Linux créée."

release-macos:
	@echo "Compilation release pour macOS..."
	GOOS=darwin GOARCH=amd64 go build -ldflags "-s -w" -o bin/$(EXECUTABLE_NAME)_macos .
	@echo "Release macOS créée."

release: clean
	@echo "Création de toutes les versions release..."
	$(MAKE) release-windows
	$(MAKE) release-linux
	$(MAKE) release-macos

clean:
	@echo "Nettoyage des fichiers binaires..."
	rm -rf bin/
	@echo "Nettoyage terminé."
