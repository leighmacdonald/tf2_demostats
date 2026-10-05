//! Typed conversions from the library model to `demostats.v1` protobuf messages.
//!
//! The JSON API flattens `Stats` into `PlayerSummary` and uses string
//! enums; the proto schema nests `stats` and uses typed enums instead.
//! These conversions are total and infallible.

use crate::demostats::v1 as pb;
use tf2_demostats::parser::{summarizer, DemoOutput};
use tf_demo_parser::demo::header::Header;
use tf_demo_parser::demo::parser::gamestateanalyser::{Class, Team};

pub fn team(value: &Team) -> pb::Team {
    match value {
        Team::Other => pb::Team::TEAM_OTHER,
        Team::Spectator => pb::Team::TEAM_SPECTATOR,
        Team::Red => pb::Team::TEAM_RED,
        Team::Blue => pb::Team::TEAM_BLUE,
    }
}

pub fn class(value: &Class) -> pb::Class {
    match value {
        Class::Other => pb::Class::CLASS_OTHER,
        Class::Scout => pb::Class::CLASS_SCOUT,
        Class::Sniper => pb::Class::CLASS_SNIPER,
        Class::Soldier => pb::Class::CLASS_SOLDIER,
        Class::Demoman => pb::Class::CLASS_DEMOMAN,
        Class::Medic => pb::Class::CLASS_MEDIC,
        Class::Heavy => pb::Class::CLASS_HEAVY,
        Class::Pyro => pb::Class::CLASS_PYRO,
        Class::Spy => pb::Class::CLASS_SPY,
        Class::Engineer => pb::Class::CLASS_ENGINEER,
    }
}

pub fn header(value: &Header) -> pb::Header {
    pb::Header {
        demo_type: value.demo_type.clone(),
        version: value.version,
        protocol: value.protocol,
        server: value.server.clone(),
        nick: value.nick.clone(),
        map: value.map.clone(),
        game: value.game.clone(),
        duration: value.duration,
        ticks: value.ticks,
        frames: value.frames,
        signon: value.signon,
        ..Default::default()
    }
}

pub fn stats(value: &tf2_demostats::parser::stats::Stats) -> pb::Stats {
    pb::Stats {
        kills: value.kills,
        assists: value.assists,
        deaths: value.deaths,
        postround_kills: value.postround_kills,
        postround_assists: value.postround_assists,
        postround_deaths: value.postround_deaths,
        preround_healing: value.preround_healing,
        healing: value.healing,
        postround_healing: value.postround_healing,
        drops: value.drops,
        near_full_charge_death: value.near_full_charge_death,
        charges_uber: value.charges_uber,
        charges_kritz: value.charges_kritz,
        charges_quickfix: value.charges_quickfix,
        damage: value.damage,
        damage_taken: value.damage_taken,
        dominations: value.dominations,
        dominated: value.dominated,
        revenges: value.revenges,
        revenged: value.revenged,
        airshots: value.airshots,
        headshot_kills: value.headshot_kills,
        backstab_kills: value.backstab_kills,
        headshots: value.headshots,
        backstabs: value.backstabs,
        captures: value.captures,
        captures_blocked: value.captures_blocked,
        was_headshot: value.was_headshot,
        was_backstabbed: value.was_backstabbed,
        shots: value.shots,
        hits: value.hits,
        object_built: value.object_built,
        object_destroyed: value.object_destroyed,
        ..Default::default()
    }
}

pub fn player(value: &tf2_demostats::parser::player::PlayerSummary) -> pb::PlayerSummary {
    let mut classes: Vec<(&Class, &tf2_demostats::parser::stats::Stats)> =
        value.classes.iter().collect();
    // Deterministic output regardless of `HashMap` iteration order.
    classes.sort_by_key(|(class, _)| **class as u8);
    let classes: Vec<pb::ClassStats> = classes
        .into_iter()
        .map(|(c, s)| pb::ClassStats {
            class: buffa::EnumValue::from(class(c)),
            stats: buffa::MessageField::some(stats(s)),
            ..Default::default()
        })
        .collect();

    pb::PlayerSummary {
        name: value.name.clone(),
        steamid: value.steamid.clone(),
        tick_start: value.tick_start.map(u32::from),
        tick_end: value.tick_end.map(u32::from),
        points: value.points,
        connection_count: value.connection_count,
        bonus_points: value.bonus_points,
        stats: buffa::MessageField::some(stats(&value.stats)),
        classes,
        weapons: value
            .weapons
            .iter()
            .map(|(name, s)| (name.clone(), stats(s)))
            .collect(),
        scoreboard_kills: value.scoreboard_kills,
        scoreboard_assists: value.scoreboard_assists,
        suicides: value.suicides,
        scoreboard_deaths: value.scoreboard_deaths,
        postround_deaths: value.postround_deaths,
        captures: value.captures,
        captures_blocked: value.captures_blocked,
        scoreboard_damage: value.scoreboard_damage,
        is_fake_player: value.is_fake_player,
        is_hl_tv: value.is_hl_tv,
        is_replay: value.is_replay,
        ..Default::default()
    }
}

pub fn round(value: &summarizer::RoundSummary) -> pb::RoundSummary {
    pb::RoundSummary {
        winner: value
            .winner
            .map(|winner| buffa::EnumValue::from(team(&winner))),
        is_stalemate: value.is_stalemate,
        is_sudden_death: value.is_sudden_death,
        time: value.time,
        mvps: value.mvps.clone(),
        players: value.players.iter().map(player).collect(),
        winners: value.winners.clone(),
        losers: value.losers.clone(),
        ..Default::default()
    }
}

pub fn chat(value: &summarizer::ChatMessage) -> pb::ChatMessage {
    pb::ChatMessage {
        tick: u32::from(value.tick),
        user: value.user.clone(),
        message: value.message.clone(),
        is_dead: value.is_dead,
        is_team: value.is_team,
        is_spec: value.is_spec,
        is_name_change: value.is_name_change,
        ..Default::default()
    }
}

pub fn vote_ballot(value: &summarizer::VoteBallot) -> pb::VoteBallot {
    pb::VoteBallot {
        tick: u32::from(value.tick),
        voter_entity: value.voter_entity,
        voter: value.voter.clone(),
        voter_name: value.voter_name.clone(),
        option: u32::from(value.option),
        option_name: value.option_name.clone(),
        ..Default::default()
    }
}

pub fn vote(value: &summarizer::VoteSummary) -> pb::VoteSummary {
    pb::VoteSummary {
        voteidx: value.voteidx,
        tick_start: u32::from(value.tick_start),
        tick_end: value.tick_end.map(u32::from),
        issue: value.issue.clone(),
        param1: value.param1.clone(),
        team: u32::from(value.team),
        initiator_entity: value.initiator_entity,
        initiator: value.initiator.clone(),
        initiator_name: value.initiator_name.clone(),
        options: value.options.clone(),
        ballots: value.ballots.iter().map(vote_ballot).collect(),
        counts: value.counts.clone(),
        potential_votes: value.potential_votes,
        passed: value.passed,
        result_details: value.result_details.clone(),
        result_param1: value.result_param1.clone(),
        ..Default::default()
    }
}

pub fn sm_initiator(value: &summarizer::SmVoteInitiator) -> pb::SmVoteInitiator {
    pb::SmVoteInitiator {
        name: value.name.clone(),
        steamid: value.steamid.clone(),
        tick: u32::from(value.tick),
        current: value.current,
        required: value.required,
        ..Default::default()
    }
}

pub fn sm_nomination(value: &summarizer::SmNomination) -> pb::SmNomination {
    pb::SmNomination {
        name: value.name.clone(),
        steamid: value.steamid.clone(),
        map: value.map.clone(),
        tick: u32::from(value.tick),
        ..Default::default()
    }
}

pub fn sm_option(value: &summarizer::SmVoteOption) -> pb::SmVoteOption {
    pb::SmVoteOption {
        name: value.name.clone(),
        votes: value.votes,
        ..Default::default()
    }
}

pub fn sm_vote(value: &summarizer::SourceModVote) -> pb::SourceModVote {
    pb::SourceModVote {
        kind: value.kind.clone(),
        tick_start: u32::from(value.tick_start),
        tick_end: value.tick_end.map(u32::from),
        initiators: value.initiators.iter().map(sm_initiator).collect(),
        nominations: value.nominations.iter().map(sm_nomination).collect(),
        total_votes: value.total_votes,
        potential_votes: value.potential_votes,
        options: value.options.iter().map(sm_option).collect(),
        result: value.result.clone(),
        passed: value.passed,
        ..Default::default()
    }
}

pub fn demo_output(value: &DemoOutput) -> pb::DemoOutput {
    pb::DemoOutput {
        filename: value.filename.clone().unwrap_or_default(),
        header: buffa::MessageField::some(header(&value.header)),
        summary: buffa::MessageField::some(pb::DemoSummary {
            rounds: value.summary.rounds.iter().map(round).collect(),
            chat: value.summary.chat.iter().map(chat).collect(),
            votes: value.summary.votes.iter().map(vote).collect(),
            sourcemod_votes: value.summary.sourcemod_votes.iter().map(sm_vote).collect(),
            ..Default::default()
        }),
        ..Default::default()
    }
}
