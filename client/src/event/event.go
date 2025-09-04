package event

import (
	"fmt"
	"io"
	"live_chat/src/constant"
	"log"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sync"
	"time"

	"github.com/ebitengine/oto/v3"
	"github.com/hajimehoshi/go-mp3"
)

func DispatchEvent(json map[string]string) {
	switch json["event"] {
	case "browser_backend":
		if _, ok := json["url"]; ok {
			openBrowser("http://" + constant.IP_ADDR_SERVER + json["url"])
		} else {
			log.Println("no url found")
		}
	case "song":
		if _, ok := json["url"]; ok {
			playSong("http://" + constant.IP_ADDR_SERVER + json["url"])
		} else {
			log.Println("no url found")
		}
	case "browser_raw":
		if _, ok := json["url"]; ok {
			openBrowser(json["url"])
		} else {
			log.Println("no url found")
		}
	case "video":
		if _, ok := json["url"]; ok {
			playVideo("http://" + constant.IP_ADDR_SERVER + json["url"], json["caption"], json["width"], json["height"])
		} else {
			log.Println("no url found")
		}
	case "combination":
		if _, ok := json["audio"]; ok {
			playSong("http://" + constant.IP_ADDR_SERVER + json["url"])
		}
		if _, ok := json["url"]; ok {
			openBrowser("http://" + constant.IP_ADDR_SERVER + json["url"])
		}
		if _, ok := json["url_raw"]; ok {
			openBrowser(json["url"])
		}
	default:
		log.Println("default:", json)
	}
}

func openBrowser(url string) {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		cmd = exec.Command("cmd", "/c", "start", url)
	case "darwin":
		cmd = exec.Command("open", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}

	err := cmd.Run()
	if err != nil {
		log.Println("Impossible d'ouvrir le navigateur:", err)
	}
}

var otoCtx *oto.Context
var mu sync.Mutex

func InitOtoContext() {
	op := &oto.NewContextOptions{}
	// Usually 44100 or 48000. Other values might cause distortions in Oto
	op.SampleRate = 44100
	// Number of channels (aka locations) to play sounds from. Either 1 or 2.
	// 1 is mono sound, and 2 is stereo (most speakers are stereo).
	op.ChannelCount = 2
	// Format of the source. go-mp3's format is signed 16bit integers.
	op.Format = oto.FormatSignedInt16LE

	otoCtxTmp, readyChan, err := oto.NewContext(op)
	if err != nil {
		panic("oto.NewContext failed: " + err.Error())
	}

	otoCtx = otoCtxTmp

	<-readyChan
}

func playSong(url string) {
	fmt.Println("Downloading sound from:", url)
	// Télécharger le fichier audio depuis l'URL
	resp, err := http.Get(url)
	if err != nil {
		log.Fatal(err)
	}

	if resp.StatusCode != http.StatusOK {
		log.Fatalf("bad status: %s", resp.Status)
	}

	decodedMp3, err := mp3.NewDecoder(resp.Body)
	if err != nil {
		panic("mp3.NewDecoder failed: " + err.Error())
	}

	if otoCtx == nil {
		mu.Lock()
		InitOtoContext()
		mu.Unlock()
	}

	player := otoCtx.NewPlayer(decodedMp3)
	player.Play()
	go func() {
		for player.IsPlaying() {
			time.Sleep(time.Millisecond)
		}
		err = player.Close()
		if err != nil {
			panic("player.Close failed: " + err.Error())
		}
		resp.Body.Close()
	}()
}

func playVideo(url string, caption string, width string, height string) {
	var cmd *exec.Cmd

	switch runtime.GOOS {
	case "windows":
		cmd = playVideoWindows(url, caption, width, height)
	case "darwin":
		cmd = exec.Command("./mpv/macos/mpv", "--fullscreen", url)
	default:
		cmd = playVideoLinux(url, caption, width, height)
	}

	if cmd == nil {
		log.Println("Command is nil")
		return
	}

	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		log.Printf("Command failed: %v", err)
		return
	}

	if runtime.GOOS == "linux" {
		wd, _ := os.Getwd()
		godotVideoPath := filepath.Join(wd, "godot_bin", "linux", "current_video.mp4")
		os.Remove(godotVideoPath)
	}
}

func playVideoWindows(url string, caption string, width string, height string) *exec.Cmd {
	exePath, err := os.Executable()
	if err != nil {
		log.Println("Impossible d'obtenir le chemin de l'exécutable:", err)
		return nil
	}
	exeDir := filepath.Dir(exePath)

	videoFile := downloadVideo(url)
	if videoFile == "" {
		log.Println("Failed to download video")
		return nil
	}

	godotVideoPath := filepath.Join(exeDir, "godot_bin", "windows", "current_video.mp4")
	err = copyFile(videoFile, godotVideoPath)
	if err != nil {
		log.Printf("Failed to copy video to godot directory: %v", err)
		return nil
	}

	godotPath := filepath.Join(exeDir, "godot_bin", "windows", "homies-video-player.exe")
	if _, err := os.Stat(godotPath); os.IsNotExist(err) {
		log.Printf("Godot executable not found: %s", godotPath)
		return nil
	}

	cmd := exec.Command(godotPath, godotVideoPath, caption, width, height)
	cmd.Dir = exeDir
	return cmd
}

func playVideoLinux(url string, caption string, width string, height string) *exec.Cmd {
	videoFile := downloadVideo(url)
	if videoFile == "" {
		log.Println("Failed to download video")
		return nil
	}

	wd, err := os.Getwd()
	if err != nil {
		log.Println("Impossible d'obtenir le répertoire de travail:", err)
		return nil
	}

	godotPath := filepath.Join(wd, "godot_bin", "linux", "homies-video-player.x86_64")
	godotVideoPath := filepath.Join(wd, "godot_bin", "linux", "current_video.mp4")
	err = copyFile(videoFile, godotVideoPath)
	if err != nil {
		log.Printf("Failed to copy video to godot directory: %v", err)
		return nil
	}

	if _, err := os.Stat(godotPath); os.IsNotExist(err) {
		log.Printf("Godot executable not found: %s", godotPath)
		return nil
	}

	cmd := exec.Command(godotPath, godotVideoPath, caption, width, height)
	cmd.Dir = filepath.Join(wd, "godot_bin", "linux")

	env := os.Environ()
	libraryPath := filepath.Join(wd, "godot_bin", "linux")
	env = append(env, "LD_LIBRARY_PATH="+libraryPath)
	cmd.Env = env

	return cmd
}

// Helper function to copy files
func copyFile(src, dst string) error {
	sourceFile, err := os.Open(src)
	if err != nil {
		return err
	}
	defer sourceFile.Close()

	destFile, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer destFile.Close()

	_, err = io.Copy(destFile, sourceFile)
	if err != nil {
		return err
	}

	return destFile.Sync()
}

func downloadVideo(url string) string {
	tempDir := os.TempDir()
	tmpFile, err := os.CreateTemp(tempDir, "video_*.mp4")
	if err != nil {
		log.Println("Failed to create temp file:", err)
		return ""
	}
	defer tmpFile.Close()

	client := &http.Client{
		Timeout: 300 * time.Second,
	}
	resp, err := client.Get(url)
	if err != nil {
		log.Println("Failed to download video:", err)
		return ""
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		log.Printf("Failed to download video: HTTP %d", resp.StatusCode)
		return ""
	}

	_, err = io.Copy(tmpFile, resp.Body)
	if err != nil {
		log.Println("Failed to save video:", err)
		return ""
	}

	tmpFile.Sync()
	filePath := tmpFile.Name()
	filePath = filepath.Clean(filePath)

	err = os.Chmod(filePath, 0644)
	if err != nil && runtime.GOOS != "windows" {
		log.Println("Warning: Failed to set file permissions:", err)
	}

	log.Printf("Video downloaded to: %s", filePath)
	return filePath
}
