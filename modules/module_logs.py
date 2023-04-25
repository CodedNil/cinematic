import os
import time
import tiktoken

# OpenAI pricing info
pricing = {
    "gpt-3.5-turbo": {
        "prompt_tokens_1k": 0.002,
        "completion_tokens_1k": 0.002,
    },
    "gpt-4": {
        "prompt_tokens_1k": 0.03,
        "completion_tokens_1k": 0.06,
    },
}


def log(module_name: str, data: str) -> None:
    """Log a query and response"""

    # Create logs folder if it doesn't exist
    log_file = "logs/" + module_name + ".log"
    if not os.path.exists("logs"):
        os.mkdir("logs")
    # Write to log file
    timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
    with open(log_file, os.path.isfile(log_file) and "a" or "w") as file:
        file.write(f"{timestamp} {data.encode('utf-8')}\n")


def log_ai(module_name: str, func: str, data: str, query: str, response: dict) -> None:
    """Log a query and response"""

    encoding = tiktoken.get_encoding("cl100k_base")
    # len(encoding.encode(string)) returns the number of tokens

    # Get relevant response info
    # Calculate cost
    model = response["model"]
    # Get prompt and completion tokens
    prompt_tokens = response["usage"]["prompt_tokens"]
    completion_tokens = response["usage"]["completion_tokens"]
    total_tokens = response["usage"]["total_tokens"]
    pricing_model = pricing["gpt-4"]
    for pmodel in pricing:
        if pmodel in model:
            pricing_model = pricing[pmodel]
            break
    cost = (
        prompt_tokens * pricing_model["prompt_tokens_1k"] / 1000
        + completion_tokens * pricing_model["completion_tokens_1k"] / 1000
    )
    responseInfo = f"prompt {prompt_tokens}; completion {completion_tokens}; total {total_tokens}; cost ${cost}"
    responseMessage = response["choices"][0]["message"]["content"].replace("\n", " ")

    # Create logs folder if it doesn't exist
    log_file = "logs/" + module_name + ".log"
    if not os.path.exists("logs"):
        os.mkdir("logs")
    # Write to log file
    timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
    with open(log_file, os.path.isfile(log_file) and "a" or "w") as file:
        file.write(
            f"{timestamp} {response['model']} {responseInfo} | {func} | {data.encode('utf-8')} | {query} -> {responseMessage}\n"
        )
