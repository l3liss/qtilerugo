package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/BurntSushi/xgb"
	"github.com/BurntSushi/xgb/xproto"
	"github.com/getlantern/systray"
	"github.com/shirou/gopsutil/v3/cpu"
	"github.com/shirou/gopsutil/v3/net"
)

// Converts uint32 slice to byte slice for X11 properties
func uint32SliceToBytes(slice []uint32) []byte {
	buf := new(bytes.Buffer)
	for _, v := range slice {
		_ = binary.Write(buf, binary.LittleEndian, v)
	}
	return buf.Bytes()
}

// Set X11 Dock properties
func setDockProperties(winID uint32, barHeight int, screenWidth int) {
	X, err := xgb.NewConn()
	if err != nil {
		log.Println("Failed to connect to X server:", err)
		return
	}
	defer X.Close()

	// Get atoms
	netWMWindowType, _ := xproto.InternAtom(X, true, uint16(len("_NET_WM_WINDOW_TYPE")), "_NET_WM_WINDOW_TYPE").Reply()
	netWMWindowTypeDock, _ := xproto.InternAtom(X, true, uint16(len("_NET_WM_WINDOW_TYPE_DOCK")), "_NET_WM_WINDOW_TYPE_DOCK").Reply()

	// Set window type to DOCK
	data := uint32SliceToBytes([]uint32{uint32(netWMWindowTypeDock.Atom)})
	_ = xproto.ChangePropertyChecked(X, xproto.PropModeReplace, xproto.Window(winID),
		netWMWindowType.Atom, xproto.AtomAtom, 32, 1, data).Check()

	// Reserve space so Qtile does not overlap the bar
	netWMStrut, _ := xproto.InternAtom(X, true, uint16(len("_NET_WM_STRUT_PARTIAL")), "_NET_WM_STRUT_PARTIAL").Reply()
	strutPartial := []uint32{
		0, 0, 0, uint32(barHeight), // left, right, bottom, top
		0, 0, 0, 0, // left_start, left_end, right_start, right_end
		0, uint32(screenWidth), // top_start, top_end
		0, 0, // bottom_start, bottom_end
	}
	data = uint32SliceToBytes(strutPartial)
	_ = xproto.ChangePropertyChecked(X, xproto.PropModeReplace, xproto.Window(winID),
		netWMStrut.Atom, xproto.AtomCardinal, 32, uint32(len(strutPartial)), data).Check()

	// Move window to (0,0)
	_ = xproto.ConfigureWindowChecked(X, xproto.Window(winID),
		xproto.ConfigWindowX|xproto.ConfigWindowY, []uint32{0, 0}).Check()
}

// scanApplications gets available .desktop applications
func scanApplications(dir string) ([]string, error) {
	var apps []string
	files, err := ioutil.ReadDir(dir)
	if err != nil {
		return apps, err
	}
	for _, file := range files {
		if !file.IsDir() && strings.HasSuffix(file.Name(), ".desktop") {
			content, err := ioutil.ReadFile(filepath.Join(dir, file.Name()))
			if err != nil {
				continue
			}
			lines := strings.Split(string(content), "\n")
			for _, line := range lines {
				if strings.HasPrefix(line, "Name=") {
					name := strings.TrimPrefix(line, "Name=")
					apps = append(apps, name)
					break
				}
			}
		}
	}
	return apps, nil
}

// System tray startup function
func onReady() {
	// Load tray icon from a PNG file
	iconData, err := os.ReadFile("/home/junktop/.config/qtile/icon.png")
	if err != nil {
		log.Println("Failed to load system tray icon:", err)
	} else {
		systray.SetIcon(iconData)
	}

	systray.SetTitle("System Tray")
	systray.SetTooltip("Qtile Go Taskbar")

	// Example tray icons
	mSteam := systray.AddMenuItem("Steam", "Open Steam")
	mFlameshot := systray.AddMenuItem("Flameshot", "Screenshot Tool")
	mQuit := systray.AddMenuItem("Quit", "Exit")

	go func() {
		for {
			select {
			case <-mSteam.ClickedCh:
				if _, err := os.StartProcess("/usr/bin/steam", []string{}, &os.ProcAttr{}); err != nil {
					log.Println("Failed to start Steam:", err)
				}
			case <-mFlameshot.ClickedCh:
				if _, err := os.StartProcess("/usr/bin/flameshot", []string{"gui"}, &os.ProcAttr{}); err != nil {
					log.Println("Failed to start Flameshot:", err)
				}
			case <-mQuit.ClickedCh:
				systray.Quit()
			}
		}
	}()
}

func main() {
	// Start system tray in a separate goroutine
	go systray.Run(onReady, func() {})

	myApp := app.New()
	w := myApp.NewWindow("Go Taskbar")

	// Set bar size
	screenWidth := float32(1920) // Adjust as needed
	barHeight := float32(30)
	w.Resize(fyne.NewSize(screenWidth, barHeight))

	// Create widgets
	timeLabel := widget.NewLabel("Time: ")
	cpuLabel := widget.NewLabel("CPU: ")
	netLabel := widget.NewLabel("Network: ")

	// "Start Menu" button
	startMenuButton := widget.NewButton("Start Menu", func() {
		apps, err := scanApplications("/usr/share/applications")
		if err != nil {
			dialog.ShowError(err, w)
			return
		}
		list := widget.NewList(
			func() int { return len(apps) },
			func() fyne.CanvasObject { return widget.NewLabel("") },
			func(i widget.ListItemID, o fyne.CanvasObject) {
				o.(*widget.Label).SetText(apps[i])
			},
		)
		d := dialog.NewCustom("Installed Applications", "Close", container.NewVScroll(list), w)
		d.Show()
	})

	// System Tray Placeholder
	trayLabel := widget.NewLabel("🖥️ System Tray")

	// Arrange widgets horizontally
	statusBar := container.NewHBox(
		startMenuButton,
		widget.NewSeparator(),
		timeLabel,
		widget.NewSeparator(),
		cpuLabel,
		widget.NewSeparator(),
		netLabel,
		widget.NewSeparator(),
		trayLabel, // Placeholder for system tray
	)

	w.SetContent(statusBar)

	// Update stats every second
	go func() {
		for {
			timeLabel.SetText("Time: " + time.Now().Format("15:04:05"))

			// CPU Usage
			percents, _ := cpu.Percent(0, false)
			if len(percents) > 0 {
				cpuLabel.SetText(fmt.Sprintf("CPU: %.2f%%", percents[0]))
			}

			// Network Usage
			netIO, _ := net.IOCounters(false)
			if len(netIO) > 0 {
				netLabel.SetText(fmt.Sprintf("Network: ↑%d ↓%d", netIO[0].BytesSent, netIO[0].BytesRecv))
			}

			time.Sleep(time.Second)
		}
	}()

	// Show window
	w.Show()

	// Set dock properties
	if x11Win, ok := w.(interface{ X11Window() uintptr }); ok {
		go setDockProperties(uint32(x11Win.X11Window()), int(barHeight), int(screenWidth))
	}

	myApp.Run()
}

