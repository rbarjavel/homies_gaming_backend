extends Node2D

func _ready():
	var args = OS.get_cmdline_args()
	print("Command line args: ", args)
	if args.size() > 0:
		var video_path = args[0]
		play_video(video_path)
	else:
		push_error("No video file provided")
		OS.kill(OS.get_process_id())

func play_video(video_path: String):
	print("Attempting to play video: ", video_path)
	if not FileAccess.file_exists(video_path):
		push_error("File doesn't exist: ", video_path)
		OS.kill(OS.get_process_id())
		return

	# Create a container node
	var container = Node2D.new()
	container.name = "VideoContainer"
	add_child(container)
	
	# Create VideoStreamPlayer
	var video_player = VideoStreamPlayer.new()
	video_player.name = "VideoPlayer"
	container.add_child(video_player)
	
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
		var scale_x = float(max_width) / actual_width
		var scale_y = float(max_height) / actual_height
		var final_scale = min(min(scale_x, scale_y), 1.0)  # Don't upscale, only downscale
		
		# Apply scaling to video player
		video_player.scale = Vector2(final_scale, final_scale)
		
		# Center the video in the container
		video_player.position = Vector2(
			(max_width - actual_width * final_scale) / 2,
			(max_height - actual_height * final_scale) / 2
		)
		
		# Set container size
		container.position = Vector2(0, 0)
		
		print("Max window size: ", max_width, "x", max_height)
		print("Final scale: ", final_scale)
		print("Scaled size: ", actual_width * final_scale, "x", actual_height * final_scale)
		print("Video position: ", video_player.position)
		
		video_player.finished.connect(func(): 
			print("Video finished, exiting")
			OS.kill(OS.get_process_id())
		)
	else:
		push_error("Failed to load video")
		OS.kill(OS.get_process_id())
