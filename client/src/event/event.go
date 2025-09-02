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
		exePath, err := os.Executable()
		if err != nil {
			log.Println("Impossible d'obtenir le chemin de l'exécutable:", err)
			return
		}
		exeDir := filepath.Dir(exePath)
		mpvPath := filepath.Join(exeDir, "mpv", "windows", "mpv.exe")
		cmd = exec.Command(mpvPath, "--fullscreen", url)
		cmd.Dir = exeDir
	case "darwin":
		cmd = exec.Command("./mpv/macos/mpv", "--fullscreen", url)
	default: // Linux
		videoFile := downloadVideo(url)
		if videoFile == "" {
			log.Println("Failed to download video")
			return
		}

		// Get working directory
		wd, err := os.Getwd()
		if err != nil {
			log.Println("Impossible d'obtenir le répertoire de travail:", err)
			return
		}

		godotPath := filepath.Join(wd, "godot_bin", "linux", "Homies Video Player.x86_64")

		// Copy video to godot directory to ensure library access
		godotVideoPath := filepath.Join(wd, "godot_bin", "linux", "current_video.mp4")
		err = copyFile(videoFile, godotVideoPath)
		if err != nil {
			log.Printf("Failed to copy video to godot directory: %v", err)
			return
		}

		log.Println("=====================================")
		log.Println(godotPath + " " + godotVideoPath + " " + caption + " " + width + " " + height)
		log.Println("=====================================")

		if _, err := os.Stat(godotPath); os.IsNotExist(err) {
			log.Printf("Godot executable not found: %s", godotPath)
			return
		}

		cmd = exec.Command(godotPath, godotVideoPath, caption, width, height)
		cmd.Dir = filepath.Join(wd, "godot_bin", "linux")

		// Set library path properly
		env := os.Environ()
		libraryPath := filepath.Join(wd, "godot_bin", "linux")
		env = append(env, "LD_LIBRARY_PATH="+libraryPath)
		cmd.Env = env
	}

	// Add nil check
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

	// Cleanup copied video file
	if runtime.GOOS == "linux" {
		wd, _ := os.Getwd()
		godotVideoPath := filepath.Join(wd, "godot_bin", "linux", "current_video.mp4")
		os.Remove(godotVideoPath)
	}
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
	// Get system temp directory
	tempDir := os.TempDir()

	// Create temporary file with proper naming
	tmpFile, err := os.CreateTemp(tempDir, "video_*.mp4")
	if err != nil {
		log.Println("Failed to create temp file:", err)
		return ""
	}
	defer tmpFile.Close()

	// Download video
	client := &http.Client{
		Timeout: 300 * time.Second, // 5 minute timeout
	}
	resp, err := client.Get(url)
	if err != nil {
		log.Println("Failed to download video:", err)
		return ""
	}
	defer resp.Body.Close()

	// Check if response is valid
	if resp.StatusCode != http.StatusOK {
		log.Printf("Failed to download video: HTTP %d", resp.StatusCode)
		return ""
	}

	// Copy video data to temp file
	_, err = io.Copy(tmpFile, resp.Body)
	if err != nil {
		log.Println("Failed to save video:", err)
		return ""
	}

	// Ensure file is written to disk
	tmpFile.Sync()

	// Get the actual file path
	filePath := tmpFile.Name()

	// Normalize path separators for the current platform
	filePath = filepath.Clean(filePath)

	// Set appropriate permissions
	err = os.Chmod(filePath, 0644)
	if err != nil && runtime.GOOS != "windows" {
		log.Println("Warning: Failed to set file permissions:", err)
	}

	log.Printf("Video downloaded to: %s", filePath)
	return filePath
}
