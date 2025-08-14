.PHONY: all clean windows linux macos

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

clean:
	@echo "Nettoyage des fichiers binaires..."
	rm -rf bin/
	@echo "Nettoyage terminé."