fn main() {
    std::fs::read_to_string(".dev.env")
        .ok()
        .into_iter()
        .flat_map(|s| {
            s.lines()
                .flat_map(|c| (!c.starts_with('#')).then(|| c.split_once('=')))
                .flatten()
                .map(|(k, v)| std::env::set_var(k, v))
                .last()
        })
        .flat_map(|_| {
            std::iter::once(
                ["SCZ_TWITCH_PASS", "SCZ_TWITCH_NICK", "SCZ_TWITCH_CHANNEL"]
                    .iter()
                    .copied()
                    .zip(std::iter::repeat(std::env::var))
                    .map(|(key, func)| func(key).map_err(|_| format!("cannot find '{key}'")))
                    .collect::<Result<Vec<_>, _>>(),
            )
            .map(|res| {
                res.map_err(|err| Some(eprintln!("{err}")).map(|_| std::process::exit(1)))
                    .unwrap()
            })
            .last()
        })
        .map(|config| {
            config
                .into_iter()
                .zip(["PASS", "NICK", "JOIN"])
                .map(|(v, fmt)| format!("{fmt} {v}\r\n"))
        })
        .flat_map(|config| {
            std::net::TcpStream::connect("irc.chat.twitch.tv:6667").map(|stream| (stream, config))
        })
        .flat_map(|(stream, config)| {
            config
                .zip(std::iter::repeat(
                    (move |stream, line| {
                        std::io::Write::write_all(&mut &stream, line.as_bytes())
                            .and_then(|_| std::io::Write::flush(&mut &stream))
                            .map(|_| stream)
                    })
                        as fn(std::net::TcpStream, &str) -> std::io::Result<std::net::TcpStream>,
                ))
                .fold(Some(stream), |stream, (line, func)| {
                    stream.and_then(|stream| func(stream, &line).ok())
                })
        })
        .flat_map(|stream| {
            std::iter::once(std::io::BufReader::new(&stream))
                .map(std::io::BufRead::lines)
                .zip(std::iter::once(&stream))
                .zip(std::iter::once((
                    (|line, mut write| {
                        line.starts_with("PING")
                            .then(|| line.replace("PING", "PONG"))
                            .map(|line| {
                                std::io::Write::write_all(&mut write, line.as_bytes())
                                    .and_then(|_| std::io::Write::flush(&mut write))
                            })
                            .is_some()
                    }) as fn(&str, &std::net::TcpStream) -> bool,
                    (|line| {
                        std::iter::once(
                            std::iter::once(line.splitn(4, ' ').map(ToString::to_string))
                                .map(|mut iter| {
                                    (iter.next(), iter.next(), iter.next(), iter.next())
                                })
                                .filter(|(_, cmd, ..)| matches!(cmd.as_deref(), Some("PRIVMSG")))
                                .flat_map(|(user, cmd, args, data)| {
                                    user.zip(cmd).zip(args).zip(data)
                                })
                                .flat_map(|(((user, cmd), args), data)| {
                                    user.split_once('!')
                                        .map(|(c, _)| c[1..].to_string())
                                        .map(|user| (user, cmd))
                                        .map(|left| (left, args))
                                        .map(|left| (left, data[1..].to_string()))
                                })
                                .map(|(((a, _), c), d)| (a, c, d)),
                        )
                        .flatten()
                        .last()
                    }) as fn(&str) -> Option<(String, String, String)>,
                )))
                .flat_map(|((read, write), (maybe_ping, parse_pm))| {
                    read.flatten()
                        .inspect(|line| eprintln!("<- {}", line.escape_debug()))
                        .flat_map(move |line| {
                            (!maybe_ping(&line, &write))
                                .then_some(())
                                .and_then(|_| parse_pm(&line))
                        })
                        .map(move |msg| (msg, write))
                })
                .flat_map(|((user, target, data), write)| {
                    std::iter::once(
                        (|mut write, (target, sender), data, handler| {
                            handler(data, sender)
                                .map(|out| format!("PRIVMSG {target} :{out}\r\n"))
                                .map(|out| {
                                    std::io::Write::write(&mut write, out.as_bytes())
                                        .and_then(|_| std::io::Write::flush(&mut write))
                                })
                                .map(drop)
                                .unwrap_or_default()
                        })
                            as fn(
                                &std::net::TcpStream,
                                (&str, &str),
                                &str,
                                fn(&str, &str) -> Option<String>,
                            ),
                    )
                    .flat_map(|dispatch| {
                        <&[(fn(&str) -> bool, fn(&str, &str) -> Option<String>)]>::into_iter(&[
                            (
                                |data| matches!(data, "!hello"),
                                |_, sender| Some(format!("hello, {sender}!")),
                            ),
                            (
                                |data| matches!(data, "!semicolon"),
                                |_, _| Some(format!("I have plenty: \x3b \x3b and \x3b")),
                            ),
                        ])
                        .zip(std::iter::repeat(dispatch))
                        .flat_map(|((check, handler), dispatch)| {
                            check(&data).then(|| dispatch(write, (&target, &user), &data, *handler))
                        })
                    })
                    .last()
                })
                .last()
        })
        .map(drop)
        .last()
        .unwrap_or_default()
}
