use async_openai::{
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs,
        CreateChatCompletionRequestArgs, CreateChatCompletionResponse, Role,
    },
    Client as OpenAiClient,
};

/// Get example prompts for the chatbot from a query message
pub async fn get_examples(
    openai_client: &OpenAiClient,
    message: String,
) -> Option<Vec<ChatCompletionRequestMessage>> {
    let examples_data: Vec<(Vec<&str>, &str)> = vec![
        // API - Resolution api
        (
            vec![
                "wants resolution queried",
                "wants movie added",
                "wants series added",
                "wants resolution changed",
            ],
            "U:Assistant adds in 1080p (qualityProfileId 4) by default, the quality profiles are: 2=SD 3=720p 4=1080p 5=2160p 6=720p/1080p 7=Any",
        ),
        // API - Adding media
        (
            vec!["wants movie added", "wants series added", "wants media added"],
            "U:Check if media is already on server when asked to add, if multiple similar results are found, verify with user by providing detail, always store a memory for the user that they want the media",
        ),
        // API - Storing memories
        (
            vec!["wants memory stored", "shared an opinion", "wants memory updated"],
            "U:CMDRET memory_get (query=) CMD memory_update (query=) You store important information about users, which media they have requested and liked, used to create recommendations from previous likes/requests, or avoid suggesting media they have already seen",
        ),
        // API - Removing media
        (
            vec![
                "wants media deleted",
                "wants movie deleted",
                "wants series deleted",
            ],
            "U:Only admins can remove media files, when a user asks to remove media, change their memory to not requesting it, ask for a review",
        ),
        // Example - User sharing what they like
        (
            vec![
                "shared an opinion",
                "likes a movie",
                "likes a series",
                "has seen a movie",
                "has seen a series",
            ],
            "U:i really love the movie animals\nA:[CMD~memory_update~loved movie animals]Thats good I will remember.",
        ),
        (
            vec![
                "shared an opinion",
                "likes a movie",
                "likes a series",
                "has seen a movie",
                "has seen a series",
            ],
            "U:the silly cinematic universe is pretty amazing\nA:[CMD~memory_update~likes the silly cinematic universe]Ive taken note.",
        ),
        (
            vec![
                "shared an opinion",
                "likes a movie",
                "likes a series",
                "has seen a movie",
                "has seen a series",
                "reviewed media"
            ],
            "U:what is the funniest film ever\nA:It's subjective, but many consider Airplane! (1980) as one of the funniest films ever made. Have you seen it, or would you like another suggestion?\nU:I have seen it, 7/10\nA:[CMD~memory_update~rated airplane 1980 7/10]If you enjoyed Airplane!, you might also like The Naked Gun (1988), another comedy classic. Have you seen that one, or do you prefer a different type of humor?",
        ),
        (
            vec![
                "shared an opinion",
                "likes a movie",
                "likes a series",
                "has seen a movie",
                "has seen a series",
                "reviewed media"
            ],
            "U:avatar 2 was 9/10\nA:[CMD~memory_update~rated avatar 2 9/10]Ive taken note",
        ),
        // Example - User asking about their memories
        (
            vec![
                "wants memory queried",
                "user asking what they have requested",
                "user asking what movies they have added",
                "user asking what series they have added",
            ],
            "U:what movies have i requested?\nA:[CMDRET~memory_get~what movies do they want?]\nS:[RES~user wants stingate 1995 & continue 2007]\nA:You have requested Stingate 1995 and Stingate 2007",
        ),
        // Example - Lookup media
        (
            vec!["wants movie queried", "wants series queried"],
            "U:Is silly man on the server?\nA:[CMDRET~movie_lookup~Silly~(title;availability;year;wantedQuality;tmdbId;id)]II am looking this up on the server\nS:[RES~(Silly Man 1;available;2018;2160p;6655;22)(Silly Man 2;available;2020;2160p;7652;23)(Silly Man 3;available;2022;2160p;14433;45)]\nA:All 3 Silly Man movies are present on the server with a wanted quality of 4k.",
        ),
        // Example - User wants their added movies researched in detail
        (
            vec!["wants disk space queried", "wants to know about their movies"],
            "U:how much disk space are my movies taking up\nA:[CMDRET~memory_get~what movies do they want?][CMDRET~movie_lookup~all~(title;availability;year,file size)]I am looking up your movies on the server\nS:[RES~user wants stingate 1995 & continue 2007][RES~(Stingate;available;year 1995;file size 1.2gb)(Stingate Continue;available;year 2007;file size 1.2gb)(Stingate Ark;available;year 2007;file size 1.2gb)(Stingate Angel Kids;unavailable;year 2010;file size 1.2gb)]]"
        ),
        // Example - User wants movie added, already on server but user wants it on personal requests
        (
            vec!["wants movie added", "wants media added"],
            "U:add stingate\nA:Movie or series?\nU:movie\nA:[CMDRET~memory_get~wants stingate movie?][CMDRET~movie_lookup~Stingate~(title;availability;year;tmdbId;id)]Looking this up\nS:[RES~user wants stingate 1995 & continue 2007][RES~(Stingate;available;year 1995;tmdbId 2353)(Stingate Continue;available;year 2007;tmdbId 15134)(Stingate Ark;available;year 2007;tmdbId 15506)(Stingate Angel Kids;unavailable;year 2010;tmdbId 723663)]\nA:Stingate 1995 and Stingate 2007 are already on the server at your request, Ark 2007 is on at someone elses request. Angel Kids 2010 is not on the server, would you like to add it? It is a reimagining of the Stingate Pilot with altered scenes, remastered visuals etc.\nU:no, but add ark to my requests too\nA:[CMD~memory_update~wants movie stingate ark]I've memorised this",
        ),
        // Example - User wants series Silly Times from 1080p to 720p then deletes it and reviews it
        (
            vec![
                "wants media quality changed",
                "wants media deleted",
                "wants series deleted",
                "wants resolution changed",
                "wants quality queried",
            ],
            "U:silly times to 720p\nA:[CMDRET~series_lookup~Silly Times~(title;availability;year;tvdbId;wantedQuality;id)]Looking up Silly Times\nS:[RES~(Silly Times;available on the server;year 2013;tvdbId 152837;wantedQuality 1080p;id 63)]\nA:[CMD~series_put~(\"id\":63,\"qualityProfileId\":3)]Found it in 1080p, changing to 720p\nU:now delete it\nA:[CMD~memory_update~doesnt want show silly times]Removed it, did you enjoy it?\nU:yeah was good\nA:[CMD~memory_update~enjoyed show silly times]Great I will remember.",
        ),
        // Example - User wants to know what the highest rated silly cinematic universe movie is
        (
            vec!["might want information from a web search"],
            "U:what is the highest rated silly cinematic universe movie\nA:[CMDRET~web_search~whats the highest rated silly cinematic universe movie and what year did it come out]Looking up Silly Cinematic Universe\nS:[RES~Captain Silly: The First Silly Hero 2021 is often considered the best movie in the universe so far, it is considered to be a step up in quality because of its more gritty and realistic approach with intense action packed CGI]
    aptain Silly: The First Silly Hero 2021 is often considered the best movie in the universe so far, it is considered to be a step up in quality because of its more gritty and realistic approach with intense action packed CGI",
        ),
        // Example - Add first two Wizard Kid films
        (
            vec![
                "wants movie added",
                "wants media added",
                "requesting media",
                "talking about a collection of media",
            ],
            "U:add the first two wizard kid films\nA:[CMDRET~memory_get~wants any of the wizard kid movies?][CMDRET~movie_lookup~Wizard Kid~(title;availability;year;tmdbId;id)]Looking up Wizard Kid\nS:[RES~user hasnt requested any wizard kid movies][RES~(Wizard Kid and Rock;unavailable;release year 2003;tmdbId 571;id N/A)(Wizard Kid and Princess;unavailable;release year 2012;tmdbId 4321;id N/A)(Wizard Kid and Wine Glass;unavailable;release year 2007;tmdbId 574;id N/A)(Wizard Kid and Room of Whispers;unavailable;release year 2005;tmdbId 572;id N/A)(Wizard Kid and Birdy;unavailable;release year 2009;tmdbId 575;id N/A)(Wizard Kid and Sticks;unavailable;release year 2012;tmdbId 5123)]\nA:[CMD~memory_update~wants movies wizard kid rock & room of whispers][CMD~movie_post~571~4][CMD~movie_post~572~4]Both are on the way in 1080p, Ill remember you want them.",
        ),
        // Example - Change resolutions of Thunder movies
        (
            vec![
                "wants resolution queried",
                "wants resolution changed",
            ],
            "U:what resolution are the thunder movies in\nA:[CMDRET~movie_lookup~Thunder~(title;availability;year;resolution;wantedQuality;tmdbId;id)]Ill check\nS:[RES~(Thunder;available;release year 2012;resolution 3840x1636;wantedQuality 1080p;tmdbId 13145;id 5)(Thunder Love;available;release year 2021;resolution 3840x1608;wantedQuality 2160p;tmdbId 656037;id 915)(Thunder Ends;available;release year 2018;resolution 3840x1600;wantedQuality 2160p;tmdbId 244053;id 17)(Thunder Darkness;available;release year 2014;resolution 3840x1600;wantedQuality 2160p;tmdbId 72338;id 12]\nA:Thunder 2012, The Dark World 2014, Ends 2018 & Love 2021 are all on in 4k, though the 2012 movie is wanted at 1080p.\nU:i want the first as 4k\nA:[CMD~movie_put~(\"id\":5,\"qualityProfileId\":5)]This has been updated to 4k.",
        ),
        // Example - Add movie trilogy Lady of the Necklace
        (
            vec![
                "wants multiple movies added",
                "talking about a collection of media",
                "requesting media",
                "wants media added",
                "wants movie added",
            ],
            "U:add lotn trilogy\nA:[CMDRET~memory_get~wants lady of the necklace trilogy?][CMDRET~movie_lookup~Lady of the Necklace~(title;availability;year;tmdbId;id)]I'm checking the server, Ive noted that you want these movies\nS:[RES~no memories][RES~(Lady of the Necklace: The Two Cottages;available;2005;8833;125)(Lady of the Necklace: The Sisterhood of the Necklace;available;2004;8834;120)(Lady of the Necklace: Queens Return;available;2006;939;126)]\nA:[CMD~memory_update~wants movies lady of the necklace trilogy]Lady of the Necklace trilogy are already on, I've noted you want them",
        ),
        // Example - Add series Eastworld
        (
            vec![
                "wants series added",
                "wants media added",
                "wants movie added",
                "requesting media",
            ],
            "U:add eastworld\nA:The movie or the series?\nU:the series\nA:[CMDRET~memory_get~wants series eastworld?][CMDRET~series_lookup~Eastworld~(title;availability;year;tvdbId;wantedQuality;id)]Looking up Eastworld\nS:[RES~user wants the Eastworld series][RES~(Eastworld;unavailable;year 2014;tvdbId 152347;wantedQuality N/A;id N/A)]\nA:[CMD~series_post~152347~4]Eastworld is on the way in 1080p",
        ),
        // Example - Check resolutions of silly cinematic universe, wants them all
        (
            vec![
                "wants movies added",
                "wants series added",
                "wants resolution queried",
                "might want information from a web search",
                "asking about a collection of media",
            ],
            "U:what resolution are the silly cinematic universe movies, i want them all\nA:[CMD~memory_update~wants movies in silly cinematic universe][CMDRET~movie_lookup~SCU~(title;availability;year;resolution;wantedQuality;tmdbId;id)]I'm looking this up, I have noted that you want them\nS:[RES~No results]\nA:[CMDRET~web_search~list Silly Cinematic Universe movies in release order with year]My first search was unsucessful I am trying again\nS:[RES~Here is a list of all Silly Cinematic Universe movies in order: Silly Man 2018; Silly Man 2 2020; Thunder 2021; Captain Silly: The First Silly Hero 2021; Silly Man 3 2022]\nA:[CMDRET~movie_lookup~Silly Man¬Thunder¬Captain Silly~(title;availability;year;wantedQuality;tmdbId;id)]I found 5 movies in the Silly Cinematic Universe, I am looking up their quality and availability on the server\nS: [RES~(Silly Man 1;available;2018;2160p;6655;22)(Silly Man 2;available;2020;2160p;7652;23)(Silly Man 3;available;2022;2160p;14433;45)][RES~(Thunder 1;available;2021;2160p;8775;24)][RES~(Captain Silly: The First Silly Hero;available;2021;2160p;12995;25)]\nA:All 5 movies are present in the server with a wanted quality of 4k.",
        ),
        // Example - Recommend me media
        (
            vec!["asking for a recommendation"],
            "U:recommend me a movie\nA:[CMDRET~memory_get~what movies does user like?]I'm looking up my memories of what you like\nS:[RES~user likes all the scu movies, action movies, and the lady of the necklace trilogy]\nA:Based on your past likes, you might love the Wizard Kid movies, there are 7 of them, is there any genre you are currently in the mood for?",
        ),
        // Example - What are the top movies on the server
        (
            vec![
                "asking for a recommendation",
                "asking what is on the server",
                "asking what is available",
                "asking for every movie on the server",
            ],
            "U:what are the top movies on the server\nA:[CMDRET~movie_lookup~all~(title;availability;year)]I'm looking this up",
        ),
    ];

    // Get all the example queries
    let mut queries: Vec<String> = Vec::new();
    for example in &examples_data {
        for query in &example.0 {
            if !queries.contains(&query.to_string()) {
                queries.push(query.to_string());
            }
        }
    }

    // Search with gpt through the example prompts for relevant ones
    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-4")
        .messages([
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content("You get provided with a users message, and a list of queries that the message could match, you need to choose which examples are relevant, if its only potentially relevant still include it, return a list of the relevant examples separated by ; Examples: what resolution is silly man? wants resolution queried;asking about a collection of media;queries resolution")
                .build().unwrap(),
            ChatCompletionRequestMessageArgs::default()
                .role(Role::User)
                .content(format!("queries: {}", queries.join(", ")))
                .build().unwrap(),
                ChatCompletionRequestMessageArgs::default()
                    .role(Role::User)
                    .content(message)
                    .build().unwrap(),
        ])
        .build().unwrap();

    // Retry the request if it fails
    let mut tries = 0;
    let response = loop {
        let response = openai_client.chat().create(request.clone()).await;
        if let Ok(response) = response {
            break Ok(response);
        } else {
            tries += 1;
            if tries >= 3 {
                break response;
            }
        }
    };
    // Return from errors
    if let Err(error) = response {
        println!("Error: {:?}", error);
        return None;
    }
    // TODO log the openai call and response
    let response: CreateChatCompletionResponse = response.unwrap();

    // Gather the examples which are relevant, add their prompts to the example prompt
    let mut return_prompts: Vec<ChatCompletionRequestMessage> = Vec::new();

    // Get the examples from the response, response message will be the examples separated by ; trim each example
    let response_examples: Vec<String> = response
        .choices
        .first()
        .unwrap()
        .message
        .content
        .split(";")
        .map(|item| item.trim().to_string())
        .collect();
    if response_examples.len() == 0 {
        return None;
    }
    for example in &examples_data {
        for query in &example.0 {
            if response_examples.contains(&query.to_lowercase().trim().to_string()) {
                // Go through the prompt split by newline, add each line as a prompt
                for line in example.1.split("\n") {
                    // Get the role of the line
                    let line_role: Option<Role> = match line {
                        _ if line.starts_with("U:") => Some(Role::User),
                        _ if line.starts_with("A:") => Some(Role::Assistant),
                        _ if line.starts_with("S:") => Some(Role::System),
                        _ => None,
                    };
                    // If the line has a role, add it to the prompts
                    if let Some(role) = line_role {
                        return_prompts.push(
                            ChatCompletionRequestMessageArgs::default()
                                .role(role)
                                .content(line[2..].to_string())
                                .build()
                                .unwrap(),
                        );
                    }
                }
                // Only add the example max once
                break;
            }
        }
    }

    // Add the messages making clear it is an example
    return_prompts.push(
        ChatCompletionRequestMessageArgs::default()
            .role(Role::User)
            .content("The above are examples, you make replies more themed with personality, do you understand?")
            .build()
            .unwrap(),
    );
    return_prompts.push(
        ChatCompletionRequestMessageArgs::default()
            .role(Role::Assistant)
            .content("I understand, the above are not real conversations only for me to learn how to format responses, I will always prompt for new information 📰, I will be helpful and informative in my real responses, often adding emojis to my responses, I'll usually end my responses with a followup question such as 'what did you think of it?'")
            .build()
            .unwrap(),
    );

    return Some(return_prompts);
}
