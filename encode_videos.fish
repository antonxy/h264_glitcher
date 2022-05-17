pushd videos_orig
for f in (find .)
	if test -f $f
		set_color purple
		echo Processing $f
		set_color normal

		set outdir ../videos_conv/(dirname $f)
		set outfile $outdir/(basename $f .mp4).h264

		mkdir -p $outdir

		ffmpeg -i $f -c:v libx264 -vf format=yuv420p,scale=1920:1080 -qp 30 -x264-params bframes=0:ref=1:min-keyint=infinite:cabac=1:scenecut=0 $outfile
	end
end
popd
