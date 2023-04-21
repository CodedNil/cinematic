import os

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


class ModuleLogs:
    """Class that modules use to log their AI calls"""

    def __init__(self, module_name: str) -> None:
        """Initialise with module name"""
        self.module_name = module_name
        self.log_file = "logs/" + module_name + ".log"

    def log(self, func: str, data: str, query: str, response: dict) -> None:
        """Log a query and response"""

        # Get relevant response info
        # Calculate cost
        model = response["model"]
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
        responseMessage = response["choices"][0]["message"]["content"].replace(
            "\n", " "
        )

        # Create logs folder if it doesn't exist
        if not os.path.exists("logs"):
            os.mkdir("logs")
        # Write to log file
        with open(self.log_file, os.path.isfile(self.log_file) and "a" or "w") as file:
            file.write(
                f"{response['model']} {responseInfo} | {func} | {data.encode('utf-8')} | {query} -> {responseMessage}\n"
            )
