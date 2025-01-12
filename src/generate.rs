use crate::{
    log_error,
    opt::{Cdn, Command, GenerateCommand, Opt, Quality},
    stream::{Game, LazyStream},
    VERSION,
};
use async_std::{fs, process, task};
use chrono::Local;
use failure::Error;
use std::path::PathBuf;

pub fn run(opts: Opt) {
    task::block_on(async {
        if let Err(e) = process(opts).await {
            log_error(&e);
            process::exit(1);
        };
    });
}

async fn process(opts: Opt) -> Result<(), Error> {
    if let Command::Generate { command } = &opts.command {
        match command {
            GenerateCommand::Xmltv { .. } => {
                println!("Creating .m3u & .xml for XMLTV...");
            }
            _ => println!("Creating playlist file..."),
        }
    }

    let mut lazy_stream = LazyStream::new(&opts).await?;

    if let Some(quality) = &opts.quality {
        lazy_stream
            .resolve_with_quality_link(&opts.cdn, quality)
            .await;
    } else {
        lazy_stream.resolve_with_master_link(&opts.cdn).await;
    }

    let games = lazy_stream.games();

    if let Command::Generate { command } = opts.command {
        match command {
            GenerateCommand::Xmltv {
                file,
                start_channel,
            } => {
                let path = file.with_extension("m3u");
                create_playlist(
                    path.clone(),
                    games.clone(),
                    &opts.cdn,
                    &opts.quality,
                    true,
                    start_channel,
                )
                .await?;

                let path = path.with_extension("xml");
                create_xmltv(path, games, &opts.cdn, &opts.quality, start_channel).await?;
            }
            GenerateCommand::Playlist { file } => {
                let path = file.with_extension("m3u");
                create_playlist(path, games, &opts.cdn, &opts.quality, false, 1000).await?;
            }
        }
    }

    Ok(())
}

async fn create_playlist(
    path: PathBuf,
    mut games: Vec<Game>,
    cdn: &Cdn,
    quality: &Option<Quality>,
    is_xmltv: bool,
    start_channel: u32,
) -> Result<(), Error> {
    let mut m3u = String::new();
    m3u.push_str("#EXTM3U\n");

    let mut id: u32 = 0;
    for game in games.iter_mut() {
        for (_, stream) in game.streams.as_mut().unwrap().iter_mut() {
            let link = if let Some(quality) = quality {
                stream.quality_link(cdn, quality).await
            } else {
                stream.master_link(cdn).await
            };

            if let Ok(link) = link {
                let title = if is_xmltv {
                    format!("Lazyman {}", id + 1)
                } else {
                    format!(
                        "{} {} @ {} {}",
                        game.game_date
                            .with_timezone(&Local)
                            .time()
                            .format("%-I:%M %p")
                            .to_string(),
                        game.away_team.team_name,
                        game.home_team.team_name,
                        stream.feed_type,
                    )
                };
                let record = format!(
                    "#EXTINF:-1 CUID=\"{}\" tvg-id=\"{}\" tvg-name=\"Lazyman {}\",{}\n{}\n",
                    start_channel + id,
                    start_channel + id,
                    id + 1,
                    title,
                    link
                );
                m3u.push_str(&record);
                id += 1;
            }
        }
    }

    fs::write(&path, m3u).await?;

    println!("Playlist saved to: {:?}", path);

    Ok(())
}

async fn create_xmltv(
    path: PathBuf,
    mut games: Vec<Game>,
    cdn: &Cdn,
    quality: &Option<Quality>,
    start_channel: u32,
) -> Result<(), Error> {
    let mut xmltv = String::new();
    xmltv.push_str(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE tv SYSTEM \"xmltv.dd\">\
         \n\
         \n  <tv generator-info-name=\"lazystream\" source-info-name=\"lazystream - {}\">",
        VERSION
    ));

    let mut id: u32 = 0;
    while id < 100 {
        let record = format!(
            "\n    <channel id=\"{}\">\
             \n      <display-name>Lazyman {}</display-name>\
             \n      <icon src=\"http://home.windstream.net/dgrodecki/images/nhl/nhl_logo2.jpg\"></icon>\
             \n    </channel>",
            start_channel + id,
            id + 1
        );
        xmltv.push_str(&record);
        id += 1;
    }

    let mut id: u32 = 0;
    for game in games.iter_mut() {
        let icons = if let Some(game_cuts) = game.game_cuts().await {
            let cuts = vec![&game_cuts.cut_320_180, &game_cuts.cut_2048_1152];
            let mut icons = String::new();
            for cut in cuts {
                let icon = format!(
                    "\n      <icon src=\"{}\" width=\"{}\" height=\"{}\"></icon>",
                    cut.src, cut.width, cut.height,
                );
                icons.push_str(&icon);
            }
            icons
        } else {
            String::from("\n      <icon src=\"\"></icon>")
        };

        let description = game.description().await.unwrap_or_else(|| String::from(""));

        for (_, stream) in game.streams.as_mut().unwrap().iter_mut() {
            let link = if let Some(quality) = quality {
                stream.quality_link(cdn, quality).await
            } else {
                stream.master_link(cdn).await
            };

            if link.is_ok() {
                let start = Local::now();
                let stop = Local::now();
                let title = format!(
                    "{} {} {} @ {}",
                    game.game_date
                        .with_timezone(&Local)
                        .time()
                        .format("%-I:%M %p")
                        .to_string(),
                    stream.feed_type,
                    game.away_team.team_name,
                    game.home_team.team_name,
                );

                let record = format!(
                    "\n    <programme channel=\"{}\" start=\"{}000000 {}\" stop=\"{}235959 {}\">\
                     \n      <title lang=\"en\">{}</title>\
                     \n      <desc lang=\"en\">{}</desc>\
                     {}\
                     \n    </programme>",
                    start_channel + id,
                    start.format("%Y%m%d"),
                    start.format("%z"),
                    stop.format("%Y%m%d"),
                    stop.format("%z"),
                    title,
                    description,
                    icons,
                );
                xmltv.push_str(&record);
                id += 1;
            }
        }
    }

    xmltv.push_str("\n  </tv>");

    fs::write(&path, xmltv).await?;

    println!("Xmltv file saved to: {:?}", path);

    Ok(())
}
