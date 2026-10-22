//! 金币任务与云异环时长协议改写自 taygedo-auto-attendance, 许可见 THIRD_PARTY.md。
use super::*;
use std::collections::HashSet;

fn protocol(message: &str) -> Failure {
    Failure::new(FailureKind::Protocol, message)
}

fn count(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn minutes(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
}

fn remaining(tasks: &[Task], code: &str, limit: u64) -> u64 {
    tasks
        .iter()
        .find(|task| task.code == code)
        .map_or(limit, |task| {
            task.limit.saturating_sub(task.complete).min(limit)
        })
}

fn post(value: &Value) -> Option<Post> {
    Some(Post {
        id: id(&value["postId"]).or_else(|| id(&value["id"]))?,
        liked: value["selfOperation"]["liked"].as_bool(),
    })
}

fn query(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn progress(label: &str, done: u64, target: u64, error: Option<Failure>) -> Step {
    let message = if target == 0 {
        "今日任务已完成".into()
    } else {
        format!("本次完成 {done}/{target}")
    };
    match error {
        Some(error) if done < target => Step::optional_failure(
            label,
            Failure::new(error.kind, format!("{message}, {}", error.message)),
        ),
        _ => Step::optional(label, done >= target, message, None),
    }
}

impl Client {
    fn tasks(&self, account: &Account, access: &str) -> Result<Vec<Task>> {
        let value = self.native("/apihub/api/getUserTasks?gid=1", account, access, None)?;
        check(&value)?;
        value["data"]["task_list1"]
            .as_array()
            .ok_or_else(|| protocol("金币任务列表格式无效"))?
            .iter()
            .map(|value| {
                Ok(Task {
                    code: id(&value["code"])
                        .or_else(|| id(&value["taskKey"]))
                        .ok_or_else(|| protocol("金币任务缺少任务标识"))?,
                    complete: count(&value["completeTimes"])
                        .ok_or_else(|| protocol("金币任务完成次数无效"))?,
                    limit: count(&value["limitTimes"])
                        .ok_or_else(|| protocol("金币任务次数上限无效"))?,
                })
            })
            .collect()
    }

    fn posts(&self, account: &Account, access: &str) -> Result<Vec<Post>> {
        let value = self.native(
            "/bbs/api/getRecommendPostList?communityId=2&count=20&page=1",
            account,
            access,
            None,
        )?;
        check(&value)?;
        let data = &value["data"];
        let list = data
            .as_array()
            .or_else(|| data["list"].as_array())
            .or_else(|| data["posts"].as_array())
            .ok_or_else(|| protocol("推荐帖子列表格式无效"))?;
        let mut seen = HashSet::new();
        Ok(list
            .iter()
            .filter_map(post)
            .filter(|p| seen.insert(p.id.clone()))
            .take(20)
            .collect())
    }

    fn post_full(&self, account: &Account, access: &str, post_id: &str) -> Result<Post> {
        let value = self.native(
            &format!("/bbs/api/getPostFull?postId={}", query(post_id)),
            account,
            access,
            None,
        )?;
        check(&value)?;
        let data = &value["data"];
        post(data)
            .or_else(|| {
                let nested = data["post"].as_object()?;
                Some(Post {
                    id: nested
                        .get("postId")
                        .and_then(id)
                        .or_else(|| nested.get("id").and_then(id))
                        .unwrap_or_else(|| post_id.into()),
                    liked: data["post"]["selfOperation"]["liked"]
                        .as_bool()
                        .or_else(|| data["selfOperation"]["liked"].as_bool()),
                })
            })
            .ok_or_else(|| protocol("帖子详情格式无效"))
    }

    fn coin_actions(&self, account: &Account, access: &str, steps: &mut Vec<Step>) -> Result<()> {
        let tasks = self.tasks(account, access)?;
        let bbs = remaining(&tasks, "signin_c", 1);
        if bbs > 0 {
            let result = self
                .native(
                    "/apihub/api/signin",
                    account,
                    access,
                    Some(form(&[("communityId", "2")])),
                )
                .and_then(|v| if already(&v) { Ok(()) } else { check(&v) });
            steps.push(match result {
                Ok(()) => progress("BBS 金币签到", 1, bbs, None),
                Err(error) => Step::optional_failure("BBS 金币签到", error),
            });
            if steps.last().is_some_and(|s| {
                matches!(s.kind, Some(FailureKind::Auth | FailureKind::Verification))
            }) {
                return Ok(());
            }
        } else {
            steps.push(progress("BBS 金币签到", 0, 0, None));
        }
        let browse = remaining(&tasks, "browse_post_c", 5);
        let like = remaining(&tasks, "like_post_c", 5);
        let share = remaining(&tasks, "share", 1);
        let posts = if browse + like + share > 0 {
            self.posts(account, access)?
        } else {
            vec![]
        };
        let mut browsed = Vec::new();
        let mut error = None;
        for p in &posts {
            if browsed.len() as u64 >= browse {
                break;
            }
            self.pause(700, 1500);
            match self.post_full(account, access, &p.id) {
                Ok(p) => browsed.push(p),
                Err(e) => {
                    let stop = e.needs_login();
                    error = Some(e);
                    if stop {
                        break;
                    }
                }
            }
        }
        let stop = error.as_ref().is_some_and(Failure::needs_login);
        steps.push(progress("浏览帖子", browsed.len() as u64, browse, error));
        if stop {
            return Ok(());
        }
        let mut seen = HashSet::new();
        let mut liked = 0;
        let mut error = None;
        // 详情中的点赞状态优先于推荐列表, 避免重复点赞取消已有点赞。
        for p in browsed.iter().chain(posts.iter()) {
            if liked >= like {
                break;
            }
            if !seen.insert(&p.id) || p.liked == Some(true) {
                continue;
            }
            if p.liked.is_none() {
                match self.post_full(account, access, &p.id) {
                    Ok(full) if full.liked == Some(false) => {}
                    Ok(_) => continue,
                    Err(e) => {
                        let stop = e.needs_login();
                        error = Some(e);
                        if stop {
                            break;
                        }
                        continue;
                    }
                }
            }
            self.pause(500, 1000);
            match self
                .native(
                    "/bbs/api/post/like",
                    account,
                    access,
                    Some(form(&[("postId", &p.id)])),
                )
                .and_then(|v| check(&v))
            {
                Ok(()) => liked += 1,
                Err(e) => {
                    let stop = e.needs_login();
                    error = Some(e);
                    if stop {
                        break;
                    }
                }
            }
        }
        let stop = error.as_ref().is_some_and(Failure::needs_login);
        steps.push(progress("点赞帖子", liked, like, error));
        if stop {
            return Ok(());
        }
        let result = if share > 0 {
            match browsed.first().or_else(|| posts.first()) {
                Some(p) => self
                    .native(
                        "/bbs/api/post/share",
                        account,
                        access,
                        Some(form(&[("platform", SHARE_PLATFORM), ("postId", &p.id)])),
                    )
                    .and_then(|v| check(&v)),
                None => Err(protocol("没有可分享的推荐帖子")),
            }
        } else {
            Ok(())
        };
        steps.push(match result {
            Ok(()) => progress("分享帖子", share, share, None),
            Err(e) => Step::optional_failure("分享帖子", e),
        });
        Ok(())
    }

    pub(in crate::attendance) fn coin_tasks(&self, account: &Account, access: &str) -> Vec<Step> {
        let mut steps = Vec::new();
        if let Err(e) = self.coin_actions(account, access, &mut steps) {
            steps.push(Step::optional_failure("金币任务", e));
        }
        let state = (|| -> Result<String> {
            let value = self.native("/apihub/api/getUserCoinTaskState", account, access, None)?;
            check(&value)?;
            let today =
                count(&value["data"]["todayCoin"]).ok_or_else(|| protocol("今日金币状态无效"))?;
            Ok(match count(&value["data"]["limitCoin"]) {
                Some(limit) => format!("今日金币 {today}/{limit}"),
                None => format!("今日金币 {today}"),
            })
        })();
        steps.push(match state {
            Ok(message) => Step::optional("今日金币", true, message, None),
            Err(e) => Step::optional_failure("今日金币", e),
        });
        steps
    }

    pub(in crate::attendance) fn cloud_duration(&self, account: &Account) -> Result<Step> {
        let body = cloud_body(account, now());
        let value = self.request_raw(
            &format!("{}/cloud/game/getUserInfo", self.laohu),
            &[
                ("Content-Type", "application/x-www-form-urlencoded".into()),
                ("User-Agent", "okhttp/3.12.1".into()),
            ],
            Some(&body),
        )?;
        check(&value)?;
        if !value["result"].is_object() {
            return Err(protocol("云异环时长响应格式无效"));
        }
        let gave = minutes(&value["result"]["perDayFirstLoginGiveDuration"]).unwrap_or(0.0);
        let remained = minutes(&value["result"]["remainedDuration"]);
        Ok(Step::optional(
            "云异环时长",
            true,
            match remained {
                Some(remained) => format!("本次赠送 {gave} 分钟, 剩余 {remained} 分钟"),
                None => format!("本次赠送 {gave} 分钟, 剩余时长未知"),
            },
            None,
        ))
    }
}

fn cloud_body(account: &Account, at: u64) -> BTreeMap<String, String> {
    let mut body = form(&[
        ("appId", CLOUD_APP_ID),
        ("channelId", CLOUD_CHANNEL_ID),
        ("bid", CLOUD_BID),
        ("sdkVersion", CLOUD_SDK_VERSION),
        ("version", CLOUD_APP_VERSION),
        ("deviceId", &account.device_id),
        ("deviceType", "Pixel 8"),
        ("deviceName", "Pixel 8"),
        ("deviceModel", "Pixel 8"),
        ("deviceSys", "14"),
        ("network", "wifi"),
        ("provider", "0"),
        ("idfa", ""),
        ("userId", &account.laohu_user_id),
        ("token", &account.laohu_token),
        ("t", &at.to_string()),
    ]);
    body.insert(
        "sign".into(),
        hash(format!(
            "{}{CLOUD_APP_KEY}",
            body.values().cloned().collect::<String>()
        )),
    );
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    #[test]
    fn coin_and_cloud_wire_requests_use_their_respective_profiles() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut requests = Vec::new();
            while requests.len() < 8 && std::time::Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap().to_string();
                let mut headers = BTreeMap::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    let (key, value) = line.trim().split_once(':').unwrap();
                    headers.insert(key.to_ascii_lowercase(), value.trim().to_string());
                }
                let len = headers
                    .get("content-length")
                    .map_or(0, |s| s.parse::<usize>().unwrap());
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                let response = match requests.len() {
                    0 => {
                        r#"{"code":0,"data":{"task_list1":[{"code":"browse_post_c","completeTimes":4,"limitTimes":5},{"code":"like_post_c","completeTimes":4,"limitTimes":5}]}}"#
                    }
                    2 => {
                        r#"{"code":0,"data":{"list":[{"postId":"post-1","selfOperation":{"liked":false}}]}}"#
                    }
                    3 => r#"{"code":0,"data":{"postId":"post-1","selfOperation":{"liked":false}}}"#,
                    6 => r#"{"code":0,"data":{"todayCoin":110,"limitCoin":150}}"#,
                    7 => {
                        r#"{"code":0,"result":{"perDayFirstLoginGiveDuration":"15","remainedDuration":120}}"#
                    }
                    _ => r#"{"code":0}"#,
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .unwrap();
                requests.push((path, headers, String::from_utf8(body).unwrap()));
            }
            requests
        });
        let account = Account {
            phone: "13800138000".into(),
            password: None,
            uid: "uid".into(),
            device_id: "device".into(),
            refresh_token: "refresh".into(),
            laohu_token: "laohu-token".into(),
            laohu_user_id: "user".into(),
        };
        let client = Client::new(&base, &base).without_delays();
        assert!(client.coin_tasks(&account, "access").iter().all(|s| s.ok));
        assert_eq!(
            client.cloud_duration(&account).unwrap().message,
            "本次赠送 15 分钟, 剩余 120 分钟"
        );
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 8);
        assert_eq!(requests[0].0, "/apihub/api/getUserTasks?gid=1");
        assert_eq!(requests[1].2, "communityId=2");
        assert_eq!(
            requests[2].0,
            "/bbs/api/getRecommendPostList?communityId=2&count=20&page=1"
        );
        assert_eq!(requests[3].0, "/bbs/api/getPostFull?postId=post-1");
        assert_eq!(requests[4].0, "/bbs/api/post/like");
        assert_eq!(requests[4].2, "postId=post-1");
        assert_eq!(requests[5].0, "/bbs/api/post/share");
        assert_eq!(requests[5].2, "platform=qq&postId=post-1");
        for (_, headers, _) in &requests[..6] {
            assert_eq!(headers["authorization"], "access");
            assert_eq!(headers["appversion"], "1.2.5");
            assert_eq!(headers["uid"], "uid");
            assert_eq!(headers["deviceid"], "device");
            assert!(headers.contains_key("ds"));
        }
        let (path, headers, _) = &requests[6];
        assert_eq!(path, "/apihub/api/getUserCoinTaskState");
        assert_eq!(headers["authorization"], "access");
        assert_eq!(headers["user-agent"], H5_USER_AGENT);
        assert_eq!(headers["origin"], "https://webstatic.tajiduo.com");
        assert!(!headers.contains_key("ds"));
        let (path, headers, body) = &requests[7];
        assert_eq!(path, "/cloud/game/getUserInfo");
        assert_eq!(headers["user-agent"], "okhttp/3.12.1");
        assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
        assert!(!headers.contains_key("authorization"));
        let mut fields: BTreeMap<_, _> = body
            .split('&')
            .map(|f| {
                let (key, value) = f.split_once('=').unwrap();
                (key.to_string(), value.replace('+', " "))
            })
            .collect();
        assert_eq!(fields["appId"], "10597");
        assert_eq!(fields["channelId"], "9");
        assert_eq!(fields["bid"], "com.pwrd.cloud.yh.laohu");
        assert_eq!(fields["token"], "laohu-token");
        assert_eq!(fields["userId"], "user");
        assert_eq!(fields["deviceId"], "device");
        assert_eq!(fields["idfa"], "");
        assert!(fields["t"].parse::<u64>().unwrap().abs_diff(now()) < 5);
        let signature = fields.remove("sign").unwrap();
        assert_eq!(
            signature,
            hash(format!(
                "{}f1b7f11fc3774f898e387368cce4da04",
                fields.values().cloned().collect::<String>()
            ))
        );
    }
}
