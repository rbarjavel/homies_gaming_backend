extends Node2D

func _ready():
	# Set viewport background to transparent
	get_tree().root.get_viewport().transparent_bg = true
	
	var args = OS.get_cmdline_args()
	print("Command line args: ", args)
	if args.size() > 0:
		var video_path = args[0]
		var caption_text = ""
		if args.size() > 1:
			caption_text = args[1]
		play_video(video_path, caption_text)
	else:
		push_error("No video file provided")
		OS.kill(OS.get_process_id())

func play_video(video_path: String, caption_text: String):
	print("Attempting to play video: ", video_path)
	if not FileAccess.file_exists(video_path):
		push_error("File doesn't exist: ", video_path)
		OS.kill(OS.get_process_id())
		return

	# Create VideoStreamPlayer
	var video_player = VideoStreamPlayer.new()
	video_player.name = "VideoPlayer"
	add_child(video_player)
	
	var video_stream = load(video_path)
	print("Loaded video stream: ", video_stream)
	
	if video_stream:
		video_player.stream = video_stream
		video_player.play()
		
		# Wait for video to load
		await RenderingServer.frame_post_draw
		await get_tree().process_frame
		
		# Get actual video dimensions
		var args = OS.get_cmdline_args()
		var actual_width = 1920  # default
		var actual_height = 1080 # default
		
		if args.size() >= 4:
			actual_width = args[2].to_int()
			actual_height = args[3].to_int()
		
		print("Video dimensions: ", actual_width, "x", actual_height)
		
		# Define maximum window size
		var max_width = 1280
		var max_height = 720
		
		# Calculate scale to fit within max dimensions while maintaining aspect ratio
		var scale_x = float(max_width) / float(actual_width)
		var scale_y = float(max_height) / float(actual_height)
		var final_scale = min(min(scale_x, scale_y), 1.0)  # Don't upscale, only downscale
		
		# Apply scaling to video player
		video_player.scale = Vector2(final_scale, final_scale)
		
		# Center the video in the container
		video_player.position = Vector2(
			(float(max_width) - float(actual_width) * final_scale) / 2.0,
			(float(max_height) - float(actual_height) * final_scale) / 2.0
		)
		
		# Create caption label
		if caption_text != "":
			var caption_label = RichTextLabel.new()
			caption_label.name = "CaptionLabel"
			caption_label.bbcode_enabled = true
			caption_label.add_theme_constant_override("outline_size", 5)
			caption_label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 1))  # Black outline
			caption_label.autowrap_mode = TextServer.AUTOWRAP_WORD
			
			# Load custom font
			var font = load("res://fonts/impact.ttf")
			if font:
				caption_label.add_theme_font_override("normal_font", font)
				caption_label.add_theme_font_override("bold_font", font)
				caption_label.add_theme_font_override("italics_font", font)
				caption_label.add_theme_font_override("bold_italics_font", font)
			
			# Calculate video display dimensions
			var video_display_width = float(actual_width) * final_scale
			var video_display_height = float(actual_height) * final_scale
			
			# Set label size to allow overflow (use max width instead of video width)
			caption_label.size = Vector2(float(max_width), 150)  # Increased height for overflow
			
			# Position at bottom center of the overall window (not just video)
			caption_label.position = Vector2(
				(float(max_width) - float(max_width)) / 2.0,  # Center in window
				video_player.position.y + video_display_height - 120.0  # Slightly below video bottom
			)
			
			# Debug information
			print("Video display size: ", video_display_width, "x", video_display_height)
			print("Video position: ", video_player.position)
			print("Label position: ", caption_label.position)
			print("Label size: ", caption_label.size)
			print("Caption text: ", caption_text)
			
			var font_size = 48  # Increased font size for better visibility
			caption_label.text = "[center][font_size=" + str(font_size) + "]" + caption_text + "[/font_size][/center]"
			
			# Add a small delay to ensure text rendering
			await get_tree().create_timer(0.1).timeout
			add_child(caption_label)
		
		print("Max window size: ", max_width, "x", max_height)
		print("Final scale: ", final_scale)
		print("Scaled size: ", float(actual_width) * final_scale, "x", float(actual_height) * final_scale)
		print("Video position: ", video_player.position)
		
		video_player.finished.connect(func(): 
			print("Video finished, exiting")
			OS.kill(OS.get_process_id())
		)
	else:
		push_error("Failed to load video")
		OS.kill(OS.get_process_id())
